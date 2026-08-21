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
          const pollRes = await this.adapter.pollGenerationProgress(msg.params?.submissionEvidence || '');
          return { id: msg.id, result: pollRes };

        case 'download_artifact':
          const dlRes = await this.adapter.downloadArtifact(msg.params.downloadUrl, msg.params.destinationPath);
          return { id: msg.id, result: dlRes };

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
      const errMsg = err?.message || String(err);
      let code = 'EXECUTION_ERROR';
      if (errMsg.startsWith('FLOW_UI_CHANGED')) {
        code = 'FLOW_UI_CHANGED';
      } else if (errMsg.startsWith('FILE_NOT_FOUND')) {
        code = 'FILE_NOT_FOUND';
      } else if (errMsg.startsWith('UPLOAD_FAILED')) {
        code = 'UPLOAD_FAILED';
      } else if (errMsg.startsWith('CLICK_FAILED')) {
        code = 'CLICK_FAILED';
      }

      return {
        id: msg.id,
        error: { code, message: errMsg },
      };
    }
  }
}
