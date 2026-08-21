import { FlowUiAdapterV1 } from './flow_adapter.js';

export interface RpcMessage {
  id: string;
  method: string;
  params?: any;
}

export interface RpcResponse {
  id: string;
  result?: any;
  error?: {
    code: string;
    message: string;
  };
}

export class FlowRpcBridge {
  private adapter = new FlowUiAdapterV1();

  async handleRpc(msg: RpcMessage): Promise<RpcResponse> {
    try {
      switch (msg.method) {
        case 'launch_browser':
          await this.adapter.launchBrowser(msg.params);
          return { id: msg.id, result: { success: true } };

        case 'navigate_to_flow':
          await this.adapter.navigateToFlow(msg.params.flowUrl);
          return { id: msg.id, result: { success: true } };

        case 'check_auth_status':
          const auth = await this.adapter.checkAuthStatus();
          return { id: msg.id, result: auth };

        case 'submit_prompt_generation':
          const submitRes = await this.adapter.submitPromptGeneration(msg.params);
          return { id: msg.id, result: submitRes };

        case 'poll_generation_progress':
          const pollRes = await this.adapter.pollGenerationProgress();
          return { id: msg.id, result: pollRes };

        case 'close_browser':
          await this.adapter.closeBrowser();
          return { id: msg.id, result: { success: true } };

        default:
          return {
            id: msg.id,
            error: { code: 'METHOD_NOT_FOUND', message: `Unknown method: ${msg.method}` },
          };
      }
    } catch (err: any) {
      return {
        id: msg.id,
        error: { code: 'EXECUTION_ERROR', message: err?.message || String(err) },
      };
    }
  }
}
