import readline from 'readline';
import { FlowRpcBridge, RpcMessage } from './bridge.js';

const bridge = new FlowRpcBridge();

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false,
});

rl.on('line', async (line: string) => {
  const trimmed = line.trim();
  if (!trimmed) return;

  try {
    const msg: RpcMessage = JSON.parse(trimmed);
    const resp = await bridge.handleRpc(msg);
    process.stdout.write(JSON.stringify(resp) + '\n');
  } catch (err: any) {
    const errResp = {
      id: 'unknown',
      error: { code: 'PARSE_ERROR', message: err?.message || 'Invalid JSON RPC message' },
    };
    process.stdout.write(JSON.stringify(errResp) + '\n');
  }
});
