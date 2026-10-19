/** An address hint only; importing it never opens a socket or authorizes trust. */
export function parseEnrollmentAddress(text: string): { host: string; port: number } {
  if (text.length > 512) throw new Error('配对地址过长');
  const url = new URL(text.trim());
  if (url.protocol !== 'capyio:' || url.hostname !== 'pair' || url.username || url.password
      || url.port || !['', '/'].includes(url.pathname) || url.hash
      || [...url.searchParams.keys()].sort().join(',') !== 'host,port,v'
      || url.searchParams.get('v') !== '3') throw new Error('不是受支持的 CapyIO 配对地址');
  const host = url.searchParams.get('host') ?? '';
  const portText = url.searchParams.get('port') ?? '';
  const port = Number(portText);
  if (!numericHost(host) || !/^[1-9][0-9]{0,4}$/.test(portText) || port > 65535) throw new Error('配对地址中的 IP 或端口无效');
  return { host, port };
}

function numericHost(host: string): boolean {
  if (/^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/.test(host)) {
    const parts = host.split('.');
    return parts.every(part => Number(part) <= 255 && String(Number(part)) === part) && Number(parts[0]) > 0 && Number(parts[0]) < 224;
  }
  if (!host.includes(':') || !/^[0-9a-f:]+$/i.test(host) || host === '::' || host.toLowerCase().startsWith('ff')) return false;
  try { return new URL(`http://[${host}]/`).hostname.length > 2; } catch { return false; }
}

export function enrollmentAddressFromEndpoint(endpoint: string): string {
  const matched = /^(?:\[([^\]]+)\]|([^:]+)):([0-9]+)$/.exec(endpoint);
  if (!matched) throw new Error('配对端点格式无效');
  const host = matched[1] ?? matched[2]!;
  const address = `capyio://pair?${new URLSearchParams({ v: '3', host, port: matched[3]! })}`;
  parseEnrollmentAddress(address);
  return address;
}
