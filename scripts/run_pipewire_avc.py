#!/usr/bin/env python3
"""Offline CAVC through production RuntimeAvcCamera and the existing camera graph.
Private synthetic graph only. Reuses the already-gated camera harness, including
sentinel continuity, explicit links, ownership and cleanup checks.
"""
import argparse
import os
from pathlib import Path
import signal
import sys
import tempfile

from run_pipewire_camera import CameraHarness

SCENARIOS = ('pixels', 'no_consumer', 'expiry', 'child_exit', 'decoder_exit',
             'server_disconnect', 'revoke', 'malformed_cavc')

class AvcHarness(CameraHarness):
    decoder_pid = None
    decoder = None
    scenario = None

    def ready_line(self, child):
        camera = super().ready_line(child)
        if not camera:
            return None
        lines = (self.root/'owner.out').read_text().splitlines()
        decoder_lines = [line for line in lines if line.startswith('DECODER_READY ')]
        if not decoder_lines and child.poll() is None:
            return None
        if len(decoder_lines) != 1:
            raise ValueError('missing or duplicate decoder identity')
        pid = int(decoder_lines[0].split()[1])
        status = dict(line.split(':', 1) for line in Path('/proc', str(pid), 'status').read_text().splitlines() if ':' in line)
        if int(status['PPid']) != child.pid or Path('/proc', str(pid), 'exe').resolve() != self.decoder.resolve():
            raise ValueError('decoder is not exact fixed sibling owned by Runtime owner')
        self.decoder_pid = pid
        if self.scenario == 'decoder_exit':
            os.kill(pid, signal.SIGTERM)
        return camera

    def run_avc(self, driver, helper, decoder, consumer, scenario):
        self.decoder, self.scenario = decoder, scenario
        super().run(driver, helper, consumer, scenario)
        if self.decoder_pid is None or Path('/proc', str(self.decoder_pid)).exists():
            raise ValueError('owned decoder not reaped after graph scenario')


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--hosted-private-avc', action='store_true', required=True)
    for name in ('driver', 'helper', 'decoder', 'consumer'):
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--scenario', choices=SCENARIOS, required=True)
    args = p.parse_args()
    with tempfile.TemporaryDirectory(prefix='capyio-avc-') as tmp:
        root=Path(tmp);root.chmod(0o700)
        h=AvcHarness(root,e2e=True)
        try:
            h.run_avc(args.driver.resolve(),args.helper.resolve(),args.decoder.resolve(),args.consumer.resolve(),args.scenario)
        except Exception:
            for child in h.children:
                print(f'avc_process name={Path(child.args[0]).name} pid={child.pid} exit={child.poll()}',file=sys.stderr)
            for name in ('daemon.err','owner.err','consumer.out','consumer.err','command.err','command.out'):
                path=root/name
                if path.exists():print(name+': '+path.read_text(errors='replace')[-4096:],file=sys.stderr)
            raise
        finally:
            h.close()
    print(f'capyio_pipewire_avc=passed scenario={args.scenario} real_graph=true owned_cleanup=true')

if __name__=='__main__':
    main()
