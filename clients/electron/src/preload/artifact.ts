// The preload of an artifact's view and export window, in every frame. It opens no door: no
// bridge, no IPC. It only takes WebRTC out of the page before the artifact's code runs.

import { contextBridge } from "electron";
import { withoutWebRtc } from "./artifact-globals";

contextBridge.executeInMainWorld({ func: withoutWebRtc });
