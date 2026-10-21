#!/usr/bin/env python3
"""Private synthetic real Android policy -> pinned TLS -> AVC -> PipeWire graph.
Reuses graph ownership, independent NV12 byte/order checks, sentinel continuity,
and child reaping checks. Partial CAVC tests the sender admission, not TLS framing.
"""
import argparse
from pathlib import Path
import sys
import tempfile


SCENARIOS = ('pixels', 'no_consumer', 'expiry', 'control_stop', 'control_eof',
             'partial_cavc', 'source_revoke', 'sink_revoke', 'child_exit', 'decoder_exit', 'server_disconnect')

from run_pipewire_avc import AvcHarness

class SecureCameraHarness(AvcHarness):
    def run_secure(self, driver, helper, decoder, consumer, scenario):
        self.run_avc(driver, helper, decoder, consumer, scenario)
        text = (self.root / 'owner.out').read_text()
        lines = set(text.splitlines())
        if 'SECURE_LISTENING no_media_children=true' not in lines:
            raise ValueError('media children were not checked before authenticated data binding')
        if scenario in ('source_revoke', 'sink_revoke', 'control_eof', 'partial_cavc') and 'INJECTED ' + scenario not in lines:
            raise ValueError('requested failure was not actually injected')
        if scenario in ('control_stop', 'sink_revoke') and 'VERIFIED ' + scenario not in lines:
            raise ValueError('expected terminal reason was not verified')
        if scenario == 'expiry' and not any(line.startswith('FINITE_GRANT_EXPIRED elapsed_ms=') for line in lines):
            raise ValueError('finite grant expiry was not reached')
        if scenario == 'no_consumer' and 'expected_backpressure=true' not in text:
            raise ValueError('secure owner did not prove bounded backpressure rejection')


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--hosted-private-secure-camera', action='store_true', required=True)
    for name in ('driver', 'helper', 'decoder', 'consumer'):
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--scenario', choices=SCENARIOS, required=True)
    args = p.parse_args()
    with tempfile.TemporaryDirectory(prefix='capyio-secure-camera-') as tmp:
        root=Path(tmp);root.chmod(0o700)
        h=SecureCameraHarness(root,e2e=True)
        try:
            h.run_secure(args.driver.resolve(),args.helper.resolve(),args.decoder.resolve(),args.consumer.resolve(),args.scenario)
        except Exception:
            for child in h.children:
                print(f'secure_camera_process name={Path(child.args[0]).name} pid={child.pid} exit={child.poll()}',file=sys.stderr)
            for name in ('daemon.err','owner.err','consumer.out','consumer.err','command.err','command.out'):
                path=root/name
                if path.exists():print(name+': '+path.read_text(errors='replace')[-4096:],file=sys.stderr)
            raise
        finally:
            h.close()
    print(f'capyio_pipewire_secure_camera=passed scenario={args.scenario} real_graph=true owned_cleanup=true')

if __name__=='__main__':
    main()
