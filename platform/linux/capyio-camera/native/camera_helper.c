/* SPDX-License-Identifier: Apache-2.0
 * Dedicated private-pipe media helper, NOT a JSON-RPC adapter control endpoint.
 * All callbacks run on the main loop: deliberately no PW_STREAM_FLAG_RT_PROCESS.
 */
#define _POSIX_C_SOURCE 200809L
#include "camera_pw.h"
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include <sys/stat.h>
struct app {
    struct pw_main_loop *loop; struct pw_context *context; struct pw_core *core;
    struct pw_stream *stream; struct spa_hook core_hook;
    struct spa_source *input,*timer,*sigint,*sigterm;
    uint8_t header[CAMERA_HEADER], *assembling,*ready;
    size_t header_used,payload_used; struct camera_header incoming,current;
    uint64_t started,last_frame,partial_started,ready_time,last_sequence,last_timestamp,lifetime,next_tick,deadline_unix_ms;
    bool have_sequence,available,discontinuity,connected,format_ok,stopping; int result;
};
static uint64_t now_ns(void) {
    struct timespec t; if(clock_gettime(CLOCK_MONOTONIC,&t)<0)abort();
    return (uint64_t)t.tv_sec*UINT64_C(1000000000)+(uint64_t)t.tv_nsec;
}
static uint64_t realtime_ms(void) {
    struct timespec t;if(clock_gettime(CLOCK_REALTIME,&t)<0)abort();
    return (uint64_t)t.tv_sec*UINT64_C(1000)+(uint64_t)t.tv_nsec/UINT64_C(1000000);
}
static void stop(struct app *a,int code) {
    if(a->stopping)return;
    a->stopping=true;a->result=code;pw_main_loop_quit(a->loop);
}
/* Check at each ingress/delivery boundary as well as on the watchdog timer. */
static bool expired(struct app *a,uint64_t n) {
    if(a->stopping)return true;
    if(n-a->started>=a->lifetime||realtime_ms()>=a->deadline_unix_ms){stop(a,0);return true;}
    if(n-a->last_frame>=CAMERA_IDLE_NS ||
       (a->partial_started&&n-a->partial_started>=CAMERA_PARTIAL_NS)) {
        stop(a,2);return true;
    }
    return false;
}
static void signal_stop(void *v,int sig) { (void)sig; stop(v,0); }
static void core_error(void *v,uint32_t id,int seq,int res,const char *message) {
    (void)seq;(void)message; if(id==PW_ID_CORE && res<0)stop(v,3);
}
static const struct pw_core_events core_events={PW_VERSION_CORE_EVENTS,.error=core_error};
static void state(void *v,enum pw_stream_state old,enum pw_stream_state s,const char *error) {
    (void)old;(void)error; struct app *a=v;
    if(s==PW_STREAM_STATE_ERROR || (a->connected&&s==PW_STREAM_STATE_UNCONNECTED))stop(a,3);
    if(s>=PW_STREAM_STATE_PAUSED)a->connected=true;
}
static void param(void *v,uint32_t id,const struct spa_pod *p) {
    struct app *a=v; if(id!=SPA_PARAM_Format)return;
    a->format_ok=false; if(!p)return;
    if(!camera_format_valid(p)||camera_buffers(a->stream)<0){stop(a,4);return;}
    a->format_ok=true;
}
static void process(void *v) {
    struct app *a=v; if(expired(a,now_ns()))return;
    struct pw_buffer *p=pw_stream_dequeue_buffer(a->stream); if(!p)return;
    struct spa_buffer *b=p->buffer;
    if(b->n_datas!=1 || !b->datas[0].data || !b->datas[0].chunk || b->datas[0].maxsize<CAMERA_BYTES) {
        pw_stream_queue_buffer(a->stream,p); stop(a,4);return;
    }
    struct spa_data *d=&b->datas[0]; struct spa_meta_header *h=spa_buffer_find_meta_data(b,SPA_META_Header,sizeof(*h));
    d->chunk->offset=0; d->chunk->stride=CAMERA_WIDTH; d->chunk->size=0;
    d->chunk->flags=SPA_CHUNK_FLAG_EMPTY;
    if(h){ memset(h,0,sizeof(*h)); h->pts=-1; h->flags=SPA_META_HEADER_FLAG_GAP; }
    if(a->available&&now_ns()-a->ready_time>CAMERA_MAX_AGE_NS){a->available=false;a->discontinuity=true;}
    if(a->format_ok&&a->available&&!expired(a,now_ns())) {
        memcpy(d->data,a->ready,CAMERA_BYTES); d->chunk->size=CAMERA_BYTES; d->chunk->flags=0;
        if(h){h->seq=a->current.sequence;h->pts=(int64_t)a->current.timestamp;
            h->flags=(a->discontinuity||a->current.flags)?SPA_META_HEADER_FLAG_DISCONT:0;}
        a->available=false;a->discontinuity=false;
    }
    pw_stream_queue_buffer(a->stream,p);
}
static const struct pw_stream_events events={PW_VERSION_STREAM_EVENTS,.state_changed=state,.param_changed=param,.process=process};
static void input(void *v,int fd,uint32_t mask) {
    struct app *a=v; (void)mask;
    /* Bound each main-loop turn, including hostile continuously-readable stdin. */
    size_t budget=CAMERA_BYTES+CAMERA_HEADER;
    while(budget) {
        if(expired(a,now_ns()))return;
        bool header=a->header_used<CAMERA_HEADER;
        size_t need=header?CAMERA_HEADER-a->header_used:CAMERA_BYTES-a->payload_used;
        uint8_t *dst=header?a->header+a->header_used:a->assembling+a->payload_used;
        if(need>budget)need=budget;
        ssize_t n=read(fd,dst,need);
        if(n==0){stop(a,(a->header_used||a->payload_used)?2:0);return;}
        if(n<0){if(errno==EAGAIN||errno==EWOULDBLOCK)return;if(errno==EINTR)return;stop(a,2);return;}
        if(expired(a,now_ns()))return;
        if(!a->partial_started)a->partial_started=now_ns();
        budget-=(size_t)n;
        if(header){a->header_used+=(size_t)n;if(a->header_used==CAMERA_HEADER){
            if(!camera_decode(a->header,&a->incoming)||(a->have_sequence&&(a->incoming.sequence<=a->last_sequence||a->incoming.timestamp<=a->last_timestamp))){stop(a,2);return;}
        }} else {
            a->payload_used+=(size_t)n;
            if(a->payload_used==CAMERA_BYTES){
                if(a->available||(a->have_sequence&&a->incoming.sequence!=a->last_sequence+1))a->discontinuity=true;
                uint8_t *swap=a->ready;a->ready=a->assembling;a->assembling=swap;
                a->current=a->incoming;a->last_sequence=a->incoming.sequence;a->last_timestamp=a->incoming.timestamp;a->have_sequence=true;
                a->available=true;a->ready_time=a->last_frame=now_ns();
                a->header_used=a->payload_used=0;a->partial_started=0;
            }
        }
    }
}
static void timer(void *v,uint64_t expirations) {
    struct app *a=v;(void)expirations;uint64_t n=now_ns();
    if(expired(a,n))return;
    if(n>=a->next_tick){
        a->next_tick+=UINT64_C(33333333);
        if(a->next_tick<=n)a->next_tick=n+UINT64_C(33333333);
        if(a->format_ok)pw_stream_trigger_process(a->stream);
    }
}
int main(int argc,char **argv) {
    if(argc!=7||strcmp(argv[1],"--token")||!camera_token(argv[2])||strcmp(argv[3],"--lifetime-ms")||strcmp(argv[5],"--deadline-unix-ms"))return 64;
    char *end;errno=0;unsigned long ms=strtoul(argv[4],&end,10);
    if(errno||!*argv[4]||*end||ms<1||ms>300000)return 64;
    for(const char *p=argv[4];*p;p++)if(*p<'0'||*p>'9')return 64;
    errno=0;unsigned long long deadline=strtoull(argv[6],&end,10);
    if(errno||!*argv[6]||*end||!deadline)return 64;
    for(const char *p=argv[6];*p;p++)if(*p<'0'||*p>'9')return 64;
    uint64_t wall=realtime_ms();if(deadline<=wall)return 2;
    if(deadline-wall<ms)ms=(unsigned long)(deadline-wall);
    struct stat stdin_stat;
    if(fstat(STDIN_FILENO,&stdin_stat)<0||!S_ISFIFO(stdin_stat.st_mode))return 64;
    struct app a={0};a.deadline_unix_ms=(uint64_t)deadline;a.lifetime=(uint64_t)ms*1000000;a.discontinuity=true;a.started=a.last_frame=now_ns();a.next_tick=a.started;
    a.assembling=malloc(CAMERA_BYTES);a.ready=malloc(CAMERA_BYTES);if(!a.assembling||!a.ready){free(a.assembling);free(a.ready);return 70;}
    char name[80];snprintf(name,sizeof(name),"capyio.camera.%s",argv[2]);
    pw_init(NULL,NULL);a.loop=pw_main_loop_new(NULL);if(!a.loop)goto fail;
    a.context=pw_context_new(pw_main_loop_get_loop(a.loop),NULL,0);if(!a.context)goto fail;
    a.core=pw_context_connect(a.context,NULL,0);if(!a.core)goto fail;
    pw_core_add_listener(a.core,&a.core_hook,&core_events,&a);
    a.stream=pw_stream_new(a.core,name,pw_properties_new(
        PW_KEY_NODE_NAME,name,PW_KEY_NODE_DESCRIPTION,"CapyIO authorized camera",
        PW_KEY_MEDIA_TYPE,"Video",PW_KEY_MEDIA_CATEGORY,"Capture",PW_KEY_MEDIA_ROLE,"Camera",
        PW_KEY_MEDIA_CLASS,"Video/Source",PW_KEY_NODE_AUTOCONNECT,"false",
        PW_KEY_NODE_VIRTUAL,"true","node.want-driver","true",NULL));
    if(!a.stream)goto fail;
    struct spa_hook stream_hook;pw_stream_add_listener(a.stream,&stream_hook,&events,&a);
    uint8_t bytes[1024];struct spa_pod_builder b=SPA_POD_BUILDER_INIT(bytes,sizeof(bytes));
    const struct spa_pod *p=camera_format(&b);
    if(pw_stream_connect(a.stream,PW_DIRECTION_OUTPUT,PW_ID_ANY,
        PW_STREAM_FLAG_MAP_BUFFERS|PW_STREAM_FLAG_DRIVER,&p,1)<0)goto fail;
    int flags=fcntl(STDIN_FILENO,F_GETFL);if(flags<0||fcntl(STDIN_FILENO,F_SETFL,flags|O_NONBLOCK)<0)goto fail;
    struct pw_loop *loop=pw_main_loop_get_loop(a.loop);
    a.input=pw_loop_add_io(loop,STDIN_FILENO,SPA_IO_IN|SPA_IO_HUP|SPA_IO_ERR,false,input,&a);
    a.timer=pw_loop_add_timer(loop,timer,&a);
    a.sigint=pw_loop_add_signal(loop,SIGINT,signal_stop,&a);a.sigterm=pw_loop_add_signal(loop,SIGTERM,signal_stop,&a);
    if(!a.input||!a.timer||!a.sigint||!a.sigterm)goto fail;
    struct timespec initial={0,1},interval={0,10000000};
    if(pw_loop_update_timer(loop,a.timer,&initial,&interval,false)<0)goto fail;
    pw_main_loop_run(a.loop);goto cleanup;
fail:a.result=3;
cleanup:
    if(a.loop){struct pw_loop *l=pw_main_loop_get_loop(a.loop);
        if(a.input)pw_loop_destroy_source(l,a.input);
        if(a.timer)pw_loop_destroy_source(l,a.timer);
        if(a.sigint)pw_loop_destroy_source(l,a.sigint);
        if(a.sigterm)pw_loop_destroy_source(l,a.sigterm);}
    if(a.stream)pw_stream_destroy(a.stream);
    if(a.core)pw_core_disconnect(a.core);
    if(a.context)pw_context_destroy(a.context);
    if(a.loop)pw_main_loop_destroy(a.loop);
    pw_deinit();free(a.assembling);free(a.ready);return a.result;
}
