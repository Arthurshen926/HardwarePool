import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseEnrollmentAddress, enrollmentAddressFromEndpoint } from '../apps/desktop/src/lib/enrollment-address.ts';

test('public pairing address round-trips without a PIN or credential', () => {
  for (const endpoint of ['100.110.59.35:43194', '[fd00::1]:43194']) {
    const text = enrollmentAddressFromEndpoint(endpoint);
    const value = parseEnrollmentAddress(text);
    assert.equal(value.port, 43194);
    assert.equal(Object.keys(value).sort().join(','), 'host,port');
    assert.ok(!text.includes('pin'));
  }
  assert.equal(parseEnrollmentAddress(enrollmentAddressFromEndpoint('100.1.2.3:80')).port, 80);
});
test('invalid, duplicate or credential-bearing address text is not imported', () => {
  for (const text of ['https://example.com', 'capyio://pair?host=example.com&port=43194&v=3',
    'capyio://pair?host=100.1.2.3&port=0&v=3', 'capyio://pair?host=100.1.2.3&port=43194&v=3&pin=12345678',
    'capyio://pair?host=100.1.2.3&host=100.1.2.4&port=43194&v=3',
    'capyio://pair?host=100.1.2.3&port=43194&v=2']) assert.throws(() => parseEnrollmentAddress(text));
});
