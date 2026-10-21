/* SPDX-License-Identifier: Apache-2.0
 * Native callback tests without a PipeWire server or external nodes. */
#define main capyio_helper_test_entrypoint
#include "camera_helper.c"
#undef main
#include <assert.h>
static void put_le(uint8_t *p,uint64_t v,unsigned n){for(unsigned i=0;i<n;i++)p[i]=(uint8_t)(v>>(8*i));}
static void send_bytes(struct app *a,int r,int w,const uint8_t *p,size_t n){
    while(n){size_t chunk=n>4096?4096:n;assert(write(w,p,chunk)==(ssize_t)chunk);input(a,r,SPA_IO_IN);p+=chunk;n-=chunk;}
}
static void send_header(struct app *a,int r,int w,uint64_t seq,uint64_t pts){
    uint8_t h[32]={0};memcpy(h,"CPNV",4);put_le(h+4,1,2);put_le(h+6,32,2);
    put_le(h+8,seq,8);put_le(h+16,pts,8);put_le(h+24,CAMERA_BYTES,4);send_bytes(a,r,w,h,sizeof(h));
}
int main(void){
    pw_init(NULL,NULL);struct app a={0};a.loop=pw_main_loop_new(NULL);assert(a.loop);
    a.deadline_unix_ms=UINT64_MAX;a.started=a.last_frame=now_ns();a.lifetime=UINT64_C(300000000000);
    a.assembling=malloc(CAMERA_BYTES);a.ready=malloc(CAMERA_BYTES);assert(a.assembling&&a.ready);
    uint8_t *frame=malloc(CAMERA_BYTES);assert(frame);int fds[2];assert(pipe(fds)==0);
    assert(fcntl(fds[0],F_SETFL,O_NONBLOCK)==0);
    send_header(&a,fds[0],fds[1],1,100);assert(!a.available&&a.header_used==32);
    for(size_t i=0;i<CAMERA_BYTES;i++)frame[i]=camera_pattern(1,i);
    send_bytes(&a,fds[0],fds[1],frame,CAMERA_BYTES);
    assert(a.available&&a.current.sequence==1&&!memcmp(a.ready,frame,CAMERA_BYTES));
    send_header(&a,fds[0],fds[1],2,101);
    send_bytes(&a,fds[0],fds[1],frame,4096);
    assert(a.current.sequence==1&&a.payload_used==4096); /* partial never replaces */
    send_bytes(&a,fds[0],fds[1],frame+4096,CAMERA_BYTES-4096);
    assert(a.current.sequence==2&&a.discontinuity&&a.header_used==0&&a.payload_used==0);
    send_header(&a,fds[0],fds[1],3,101);assert(a.result==2); /* timestamp replay */
    a.header_used=a.payload_used=0;a.result=0;a.stopping=false;
    send_header(&a,fds[0],fds[1],2,102);assert(a.result==2); /* sequence replay */
    a.header_used=a.payload_used=0;a.result=0;a.stopping=false;
    a.deadline_unix_ms=UINT64_MAX;a.started=a.last_frame=now_ns();a.lifetime=UINT64_C(300000000000);
    a.partial_started=now_ns()-CAMERA_PARTIAL_NS-1;timer(&a,1);assert(a.result==2);
    a.partial_started=0;a.result=0;a.stopping=false;a.last_frame=now_ns()-CAMERA_IDLE_NS-1;timer(&a,1);assert(a.result==2);
    a.last_frame=now_ns();a.started=now_ns()-1000000;a.lifetime=1;a.result=99;a.stopping=false;timer(&a,1);assert(a.result==0);
    a.deadline_unix_ms=UINT64_MAX;a.started=a.last_frame=now_ns();a.lifetime=UINT64_C(300000000000);
    a.deadline_unix_ms=realtime_ms()-1;a.result=99;a.stopping=false;timer(&a,1);assert(a.result==0);
    a.deadline_unix_ms=UINT64_MAX;
    a.header_used=1;a.result=0;a.stopping=false;close(fds[1]);input(&a,fds[0],SPA_IO_HUP);assert(a.result==2);
    a.header_used=0;a.result=99;a.stopping=false;input(&a,fds[0],SPA_IO_HUP);assert(a.result==0);
    a.stopping=false;a.result=99;a.deadline_unix_ms=realtime_ms()-1;
    process(&a);assert(a.result==0); /* expiry before stream access */
    a.stopping=false;a.result=99;a.deadline_unix_ms=UINT64_MAX;
    a.started=a.last_frame=now_ns();a.lifetime=UINT64_C(300000000000);
    a.partial_started=now_ns()-CAMERA_PARTIAL_NS-1;
    input(&a,fds[0],SPA_IO_IN);assert(a.result==2); /* ingress deadline */
    close(fds[0]);free(frame);free(a.ready);free(a.assembling);pw_main_loop_destroy(a.loop);pw_deinit();
    puts("native assembler, replacement, monotonicity, deadline and EOF checks passed");return 0;
}
