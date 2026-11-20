// Before a run: stop the daemons and remove the folders of earlier runs that were killed.

import { cleanLeftovers } from "./daemon";

export default function globalSetup(): void {
  const removed = cleanLeftovers();
  if (removed > 0) console.log(`Removed ${removed} leftover test daemon folder(s).`);
}
