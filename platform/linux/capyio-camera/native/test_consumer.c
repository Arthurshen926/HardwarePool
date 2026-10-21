/* SPDX-License-Identifier: Apache-2.0
 * Terminal outcome regression only: no graph or delivered-pixel claim. */
#define main capyio_consumer_test_entrypoint
#include "camera_consumer.c"
#undef main
#include <assert.h>
int main(void) {
    pw_init(NULL,NULL);
    struct consumer c={.result=3};c.loop=pw_main_loop_new(NULL);assert(c.loop);
    quit(&c,0);
    state(&c,PW_STREAM_STATE_PAUSED,PW_STREAM_STATE_UNCONNECTED,NULL);
    assert(c.result==0&&c.stopping);
    c.stopping=false;c.result=3;
    state(&c,PW_STREAM_STATE_PAUSED,PW_STREAM_STATE_ERROR,"synthetic failure");
    quit(&c,0);
    assert(c.result==3&&c.stopping);
    c.stopping=false;
    quit(&c,4);quit(&c,0);
    assert(c.result==4);
    pw_main_loop_destroy(c.loop);pw_deinit();
    puts("native consumer terminal result is stable through teardown");return 0;
}
