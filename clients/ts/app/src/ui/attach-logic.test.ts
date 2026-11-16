import { describe, expect, it } from "vitest";
import { baseName, isImage, mergeAttachments, pathsOf, withFolders } from "./attach-logic";

describe("a prompt box's attachments", () => {
  it("know a photo by its type, or by its name when the type is missing", () => {
    expect(isImage({ name: "x", type: "image/png" })).toBe(true);
    expect(isImage({ name: "shot.PNG" })).toBe(true);
    expect(isImage({ name: "photo.jpeg", type: "" })).toBe(true);
    expect(isImage({ name: "drawing.svg", type: "image/svg+xml" })).toBe(false);
    expect(isImage({ name: "notes.pdf" })).toBe(false);
  });

  it("name a folder by its last part on any system", () => {
    expect(baseName("/home/a/project/")).toBe("project");
    expect(baseName("C:\\Work\\Specs")).toBe("Specs");
  });

  it("name their folders under the message, and alone when there are no words", () => {
    expect(withFolders("Look at this", [])).toBe("Look at this");
    expect(withFolders("Look at this  ", ["/a"])).toBe("Look at this\n\nFolder to use:\n- /a");
    expect(withFolders("", ["/a", "/b"])).toBe("Folders to use:\n- /a\n- /b");
  });

  it("keep each path once and at most the limit", () => {
    const a = { kind: "file" as const, path: "/a", name: "a" };
    const b = { kind: "image" as const, path: "/b", name: "b" };
    const merged = mergeAttachments([a], [a, b, { kind: "folder", path: "/c", name: "c" }], 2);
    expect(merged.map((m) => m.path)).toEqual(["/a", "/b"]);
    expect(pathsOf(merged, "image")).toEqual(["/b"]);
  });
});
