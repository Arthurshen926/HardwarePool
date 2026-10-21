#include "camera_protocol.h"
#include <assert.h>
#include <stdio.h>
static void put(uint8_t *p,uint64_t v,unsigned n){for(unsigned i=0;i<n;i++)p[i]=(uint8_t)(v>>(8*i));}
int main(void){
    uint8_t p[32]={0};struct camera_header h;
    memcpy(p,"CPNV",4);put(p+4,1,2);put(p+6,32,2);put(p+8,9,8);put(p+16,123456,8);put(p+24,CAMERA_BYTES,4);
    assert(camera_decode(p,&h)&&h.sequence==9&&h.timestamp==123456&&h.flags==0);
    p[0]='X';assert(!camera_decode(p,&h));p[0]='C';
    p[4]=2;assert(!camera_decode(p,&h));p[4]=1;
    p[6]=31;assert(!camera_decode(p,&h));p[6]=32;
    p[24]^=1;assert(!camera_decode(p,&h));p[24]^=1;
    p[28]=2;assert(!camera_decode(p,&h));p[28]=1;assert(camera_decode(p,&h));
    p[23]=128;assert(!camera_decode(p,&h));
    assert(camera_token("test-0123"));assert(!camera_token(""));assert(!camera_token("../x"));assert(!camera_token("UPPER"));
    assert(!camera_token("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    for(size_t i=0;i<CAMERA_BYTES;i++){uint8_t v=camera_pattern(123,i);assert(v>=16);assert(v<=(i<921600?235:240));}
    assert(camera_pattern(0,921600)!=camera_pattern(0,921601));
    puts("protocol and synthetic NV12 checks passed");return 0;
}
