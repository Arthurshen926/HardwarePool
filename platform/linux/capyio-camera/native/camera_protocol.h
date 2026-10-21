/* SPDX-License-Identifier: Apache-2.0 */
#ifndef CAPYIO_CAMERA_PROTOCOL_H
#define CAPYIO_CAMERA_PROTOCOL_H
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#define CAMERA_WIDTH 1280u
#define CAMERA_HEIGHT 720u
#define CAMERA_BYTES 1382400u
#define CAMERA_HEADER 32u
#define CAMERA_PARTIAL_NS UINT64_C(2000000000)
#define CAMERA_IDLE_NS UINT64_C(5000000000)
#define CAMERA_MAX_AGE_NS UINT64_C(250000000)
struct camera_header { uint64_t sequence, timestamp; uint32_t flags; };
static inline uint64_t camera_le(const uint8_t *p, unsigned n) {
    uint64_t v=0; for(unsigned i=0;i<n;i++) v|=(uint64_t)p[i]<<(8*i); return v;
}
static inline bool camera_decode(const uint8_t *p, struct camera_header *h) {
    if(memcmp(p,"CPNV",4)||camera_le(p+4,2)!=1||camera_le(p+6,2)!=32||
       camera_le(p+24,4)!=CAMERA_BYTES||(camera_le(p+28,4)&~UINT64_C(1))) return false;
    h->sequence=camera_le(p+8,8); h->timestamp=camera_le(p+16,8);
    h->flags=(uint32_t)camera_le(p+28,4); return h->timestamp<=INT64_MAX;
}
static inline bool camera_token(const char *p) {
    size_t n=strlen(p); if(!n||n>48)return false;
    for(size_t i=0;i<n;i++) if(!((p[i]>='a'&&p[i]<='z')||(p[i]>='0'&&p[i]<='9')||p[i]=='-'))return false;
    return true;
}
/* Synthetic fixtures: Y is 16..235, interleaved U/V are 16..240. */
static inline uint8_t camera_pattern(uint64_t seq, size_t i) {
    if(i<CAMERA_WIDTH*CAMERA_HEIGHT) return (uint8_t)(16+(i%CAMERA_WIDTH+3*(i/CAMERA_WIDTH)+seq%220)%220);
    size_t j=i-CAMERA_WIDTH*CAMERA_HEIGHT;
    return (uint8_t)(16+((j/2)%CAMERA_WIDTH+(j%2?97:23)+seq%225)%225);
}
#endif
