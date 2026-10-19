import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseDiscoveryInterfaces, selectDiscoveryInterface } from '../apps/desktop/src/lib/discovery-interfaces.ts';

const first = { ipv4: '192.168.1.2', interfaceIndex: 1 };
const second = { ipv4: '100.100.1.2', interfaceIndex: 2 };

test('one network defaults automatically; multiple networks require a choice', () => {
  assert.equal(selectDiscoveryInterface([], ''), '');
  assert.equal(selectDiscoveryInterface([first], ''), first.ipv4);
  assert.equal(selectDiscoveryInterface([first, second], ''), '');
  assert.equal(selectDiscoveryInterface([first, second], second.ipv4), second.ipv4);
  assert.equal(selectDiscoveryInterface([first], second.ipv4), first.ipv4);
  assert.equal(selectDiscoveryInterface([], first.ipv4), '');
});

test('readback accepts bounded local inventory without inventing a peer or grant', () => {
  assert.deepEqual(parseDiscoveryInterfaces([first, second]), [first, second]);
  assert.deepEqual(parseDiscoveryInterfaces([]), []);
  assert.deepEqual(Object.keys(parseDiscoveryInterfaces([first])[0]).sort(), ['interfaceIndex', 'ipv4']);
});

test('invalid native bridge data fails closed', () => {
  for (const value of [null, {}, [null], Array(65).fill(first), [first, first],
    [{ ...first, interfaceIndex: 0 }], [{ ...first, interfaceIndex: 1.5 }],
    [{ ...first, interfaceIndex: 0x100000000 }], [{ ...first, trusted: true }]]) {
    assert.throws(() => parseDiscoveryInterfaces(value));
  }
  for (const ipv4 of ['', 'localhost', '192.168.001.2', '192.168.1.256', '192.168.1.2:123', ' 192.168.1.2',
    '0.0.0.0', '0.1.2.3', '127.0.0.1', '224.0.0.1', '240.1.1.1', '255.255.255.255', '::1']) {
    assert.throws(() => parseDiscoveryInterfaces([{ ...first, ipv4 }]));
  }
});
