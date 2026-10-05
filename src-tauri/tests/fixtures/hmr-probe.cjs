// Native protocol acceptance helper. Never shipped with the product.
const fs = require('node:fs');
const [wsModule, caFile, url, hostname, source, content, mode] = process.argv.slice(2);
const WebSocket = require(wsModule);
const socket = new WebSocket(url, mode === 'vite' ? 'vite-hmr' : [], {
  headers: { Host: hostname, Origin: `https://${hostname}` },
  servername: hostname,
  ca: fs.readFileSync(caFile),
});
let edited = false;
const timeout = setTimeout(() => { console.error('No HMR notification after source edit'); process.exit(1); }, 20000);
socket.on('open', () => {
  if (mode === 'next') socket.send(JSON.stringify({ event: 'ping', page: '/', appDirRoute: true }));
  setTimeout(() => { edited = true; fs.writeFileSync(source, content); }, 500);
});
socket.on('message', raw => {
  let event;
  try { event = JSON.parse(raw.toString()); } catch { return; }
  const nextUpdate = ['building', 'built', 'serverComponentChanges', 'turbopack-message'].includes(event.type);
  if (edited && ((mode === 'vite' && event.type === 'update') || (mode === 'next' && nextUpdate))) {
    console.log(`HMR notification received: ${event.type}`);
    clearTimeout(timeout);
    socket.terminate();
  }
});
socket.on('error', error => { console.error(error); process.exit(1); });
