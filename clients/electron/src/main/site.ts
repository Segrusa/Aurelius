import { existsSync, statSync } from "node:fs";
import { join, normalize, relative } from "node:path";

/** The file of the site for a path. Anything else, also a way out of the site, is a screen of the app: the router's. */
export function siteFile(site: string, pathname: string): string {
  const index = join(site, "index.html");
  let decoded: string;
  try {
    decoded = decodeURIComponent(pathname);
  } catch {
    return index;
  }
  const asked = normalize(join(site, decoded));
  if (relative(site, asked).startsWith("..")) return index;
  return existsSync(asked) && statSync(asked).isFile() ? asked : index;
}
