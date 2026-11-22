// What the artifact preload takes out of every frame of an artifact, before the artifact's own code
// runs. It runs in the page's world (contextBridge.executeInMainWorld sends it as source), so it
// may not use anything from outside its own body.

/**
 * Removes WebRTC's constructors from `scope`, for good. With the artifact session WebRTC reaches
 * no host, but Chromium still resolves a TURN server's host name before it would use the proxy:
 * a DNS channel. Nothing else in an artifact needs WebRTC.
 */
export function withoutWebRtc(scope: Record<string, unknown> = globalThis as unknown as Record<string, unknown>): void {
  const names = [
    "RTCPeerConnection",
    "webkitRTCPeerConnection",
    "RTCDataChannel",
    "RTCIceCandidate",
    "RTCIceTransport",
    "RTCSessionDescription",
  ];
  for (const name of names) {
    try {
      delete scope[name];
    } catch {
      // A property that cannot be deleted is still made undefined below.
    }
    try {
      Object.defineProperty(scope, name, { value: undefined, writable: false, configurable: false, enumerable: false });
    } catch {
      // Already undefined and fixed.
    }
  }
}
