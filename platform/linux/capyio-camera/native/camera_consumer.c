/* SPDX-License-Identifier: Apache-2.0
 * Independent native test consumer. Never auto-links; link explicitly outside
 * this process. Validates actual delivered bytes, not enqueue acknowledgments.
 */
#define _POSIX_C_SOURCE 200809L
#include "camera_pw.h"
#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
struct consumer {
    struct pw_main_loop *loop;struct pw_stream *stream;
    unsigned required,seen;bool format_ok,have_last;int result;
    uint64_t last_seq,last_pts,start,last_frame;unsigned gaps;
};
static uint64_t monotonic(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return (uint64_t)t.tv_sec*1000000000+(uint64_t)t.tv_nsec;}
static void quit(struct consumer *c,int result){c->result=result;pw_main_loop_quit(c->loop);}
static void param(void *v,uint32_t id,const struct spa_pod *p){
    struct consumer *c=v;if(id!=SPA_PARAM_Format)return;c->format_ok=false;if(!p)return;
    if(!camera_format_valid(p)||camera_buffers(c->stream)<0){quit(c,4);return;}c->format_ok=true;
}
static void state(void *v,enum pw_stream_state old,enum pw_stream_state s,const char *error){
    if(s==PW_STREAM_STATE_ERROR||(old>=PW_STREAM_STATE_PAUSED&&s==PW_STREAM_STATE_UNCONNECTED)){
        fprintf(stderr,"camera_consumer_state old=%d new=%d detail=%.160s\n",old,s,error?error:"none");quit(v,3);
    }
}
static void process(void *v){
    struct consumer *c=v;struct pw_buffer *p=pw_stream_dequeue_buffer(c->stream);if(!p)return;
    struct spa_buffer *b=p->buffer;bool ok=c->format_ok&&b->n_datas==1;
    struct spa_data *d=ok?&b->datas[0]:NULL;
    struct spa_meta_header *h=spa_buffer_find_meta_data(b,SPA_META_Header,sizeof(*h));
    if(ok&&d->chunk&&d->chunk->size==0){pw_stream_queue_buffer(c->stream,p);return;}
    ok=ok&&d->data&&d->chunk&&h&&d->chunk->size==CAMERA_BYTES&&d->chunk->stride==CAMERA_WIDTH&&
        d->chunk->offset<=d->maxsize&&CAMERA_BYTES<=d->maxsize-d->chunk->offset&&h->pts>=0&&
        !(d->chunk->flags&(SPA_CHUNK_FLAG_CORRUPTED|SPA_CHUNK_FLAG_EMPTY))&&
        !(h->flags&(SPA_META_HEADER_FLAG_CORRUPTED|SPA_META_HEADER_FLAG_GAP));
    if(ok&&c->have_last){
        ok=h->seq>c->last_seq&&(uint64_t)h->pts>c->last_pts;
        if(ok&&h->seq!=c->last_seq+1){ok=(h->flags&SPA_META_HEADER_FLAG_DISCONT)!=0;c->gaps++;}
    }
    if(ok){const uint8_t *bytes=(const uint8_t *)d->data+d->chunk->offset;
        for(size_t i=0;i<CAMERA_BYTES;i++)if(bytes[i]!=camera_pattern(h->seq,i)){ok=false;break;}}
    if(ok){c->seen++;c->have_last=true;c->last_seq=h->seq;c->last_pts=(uint64_t)h->pts;c->last_frame=monotonic();}
    pw_stream_queue_buffer(c->stream,p);
    if(!ok)quit(c,4);else if(c->seen>=c->required)quit(c,0);
}
static void timeout(void *v,uint64_t n){struct consumer *c=v;(void)n;uint64_t now=monotonic();
    if(now-c->start>UINT64_C(30000000000)||now-c->last_frame>UINT64_C(10000000000))quit(c,5);}
static const struct pw_stream_events events={PW_VERSION_STREAM_EVENTS,.param_changed=param,.state_changed=state,.process=process};
int main(int argc,char **argv){
    if(argc!=5||strcmp(argv[1],"--token")||!camera_token(argv[2])||strcmp(argv[3],"--frames"))return 64;
    char *end;errno=0;unsigned long count=strtoul(argv[4],&end,10);
    if(errno||!*argv[4]||*end||count<1||count>300)return 64;
    for(const char *p=argv[4];*p;p++)if(*p<'0'||*p>'9')return 64;
    struct consumer c={.required=(unsigned)count,.result=3};pw_init(NULL,NULL);
    c.loop=pw_main_loop_new(NULL);if(!c.loop){pw_deinit();return 3;}
    char name[96];snprintf(name,sizeof(name),"capyio.camera-test.%s",argv[2]);
    c.stream=pw_stream_new_simple(pw_main_loop_get_loop(c.loop),name,pw_properties_new(
        PW_KEY_NODE_NAME,name,PW_KEY_MEDIA_TYPE,"Video",PW_KEY_MEDIA_CATEGORY,"Capture",
        PW_KEY_MEDIA_CLASS,"Stream/Input/Video",PW_KEY_NODE_AUTOCONNECT,"false",NULL),&events,&c);
    if(!c.stream)goto done;
    uint8_t bytes[1024];struct spa_pod_builder b=SPA_POD_BUILDER_INIT(bytes,sizeof(bytes));
    const struct spa_pod *p=camera_format(&b);
    if(pw_stream_connect(c.stream,PW_DIRECTION_INPUT,PW_ID_ANY,PW_STREAM_FLAG_MAP_BUFFERS,&p,1)<0)goto done;
    struct spa_source *timer=pw_loop_add_timer(pw_main_loop_get_loop(c.loop),timeout,&c);if(!timer)goto done;
    c.start=c.last_frame=monotonic();struct timespec initial={0,1},interval={0,100000000};
    if(pw_loop_update_timer(pw_main_loop_get_loop(c.loop),timer,&initial,&interval,false)>=0)pw_main_loop_run(c.loop);
    pw_loop_destroy_source(pw_main_loop_get_loop(c.loop),timer);
done:
    if(c.stream)pw_stream_destroy(c.stream);
    pw_main_loop_destroy(c.loop);pw_deinit();
    printf("{\"verified_frames\":%u,\"last_sequence\":%" PRIu64 ",\"sequence_gaps\":%u,\"result\":%d}\n",c.seen,c.last_seq,c.gaps,c.result);
    return c.result;
}
