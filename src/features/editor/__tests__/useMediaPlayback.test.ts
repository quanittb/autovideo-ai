import { describe, it, expect } from 'vitest';
import { mapMediaError } from '../hooks/useMediaPlayback';

describe('mapMediaError mapping logic', () => {
  it('maps null error to fallback MEDIA_PLAYBACK_ERROR', () => {
    const msg = mapMediaError(null);
    expect(msg).toContain('MEDIA_PLAYBACK_ERROR');
  });

  it('maps code 1 to MEDIA_ERR_ABORTED', () => {
    const msg = mapMediaError({ code: 1, message: 'aborted' });
    expect(msg).toContain('MEDIA_ERR_ABORTED');
  });

  it('maps code 2 to MEDIA_ERR_NETWORK', () => {
    const msg = mapMediaError({ code: 2, message: 'network' });
    expect(msg).toContain('MEDIA_ERR_NETWORK');
  });

  it('maps code 3 to MEDIA_DECODE_ERROR', () => {
    const msg = mapMediaError({ code: 3, message: 'decode failure' });
    expect(msg).toContain('MEDIA_DECODE_ERROR');
  });

  it('maps code 4 to MEDIA_SOURCE_NOT_SUPPORTED', () => {
    const msg = mapMediaError({ code: 4, message: 'source not supported' });
    expect(msg).toContain('MEDIA_SOURCE_NOT_SUPPORTED');
  });

  it('maps unknown error code with code number and message', () => {
    const msg = mapMediaError({ code: 99, message: 'custom failure' });
    expect(msg).toContain('code 99');
    expect(msg).toContain('custom failure');
  });
});
