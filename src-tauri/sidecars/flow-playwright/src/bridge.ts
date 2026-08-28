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

        case 'dry_run_preflight':
          const dryRunRes = await this.adapter.dryRunPreflight(msg.params);
          return { id: msg.id, result: dryRunRes };

        case 'ensure_uploaded_video_edit_active':
          const editVerif = await this.adapter.ensureUploadedVideoEditActive(msg.params);
          return { id: msg.id, result: editVerif };

        case 'read_credit_balance':
          const creditRes = await this.adapter.readCreditBalance();
          return { id: msg.id, result: creditRes };

        case 'prepare_video_edit_submission':
          const prepRes = await this.adapter.prepareVideoEditSubmission(msg.params);
          return { id: msg.id, result: prepRes };

        case 'submit_prepared_video_edit':
          const subRes = await this.adapter.submitPreparedVideoEdit(msg.params);
          return { id: msg.id, result: subRes };

        case 'recover_existing_submission':
          const recRes = await this.adapter.recoverExistingSubmission(msg.params);
          return { id: msg.id, result: recRes };

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
      } else if (errMsg.startsWith('GENERATION_AMBIGUOUS')) {
        code = 'GENERATION_AMBIGUOUS';
      } else if (errMsg.startsWith('DOWNLOAD_CONTROL_NOT_OBSERVED')) {
        code = 'DOWNLOAD_CONTROL_NOT_OBSERVED';
      } else if (errMsg.startsWith('FLOW_CONFIGURATION_UNVERIFIED')) {
        code = 'FLOW_CONFIGURATION_UNVERIFIED';
      } else if (errMsg.startsWith('FLOW_VIDEO_NOT_ATTACHED')) {
        code = 'FLOW_VIDEO_NOT_ATTACHED';
      } else if (errMsg.startsWith('FLOW_VIDEO_EDIT_NOT_ACTIVE')) {
        code = 'FLOW_VIDEO_EDIT_NOT_ACTIVE';
      } else if (errMsg.startsWith('FLOW_CREDIT_UI_POLICY_CONFLICT')) {
        code = 'FLOW_CREDIT_UI_POLICY_CONFLICT';
      } else if (errMsg.startsWith('FLOW_STALE_CREDIT_DETECTED')) {
        code = 'FLOW_STALE_CREDIT_DETECTED';
      } else if (errMsg.startsWith('SECURITY_VIOLATION')) {
        code = 'SECURITY_VIOLATION';
      }

      return {
        id: msg.id,
        error: { code, message: errMsg },
      };
    }
  }
}
