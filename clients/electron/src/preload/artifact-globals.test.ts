import { describe, expect, it } from "vitest";
import { withoutWebRtc } from "./artifact-globals";

describe("withoutWebRtc", () => {
  it("takes WebRTC's constructors out for good, and leaves the rest", () => {
    class Fake {}
    const scope: Record<string, unknown> = {
      RTCPeerConnection: Fake,
      webkitRTCPeerConnection: Fake,
      RTCDataChannel: Fake,
      RTCIceCandidate: Fake,
      RTCIceTransport: Fake,
      RTCSessionDescription: Fake,
      fetch: Fake,
    };
    withoutWebRtc(scope);
    for (const name of ["RTCPeerConnection", "webkitRTCPeerConnection", "RTCDataChannel", "RTCIceTransport"]) {
      expect(scope[name]).toBeUndefined();
      expect(() => {
        scope[name] = Fake;
      }).toThrow(TypeError);
      expect(Object.getOwnPropertyDescriptor(scope, name)?.configurable).toBe(false);
    }
    expect(scope.fetch).toBe(Fake);
  });

  it("runs on its own, as its source in another world", () => {
    const again = new Function(`return (${withoutWebRtc.toString()})`)() as typeof withoutWebRtc;
    const scope: Record<string, unknown> = { RTCPeerConnection: class {} };
    again(scope);
    expect(scope.RTCPeerConnection).toBeUndefined();
  });
});
