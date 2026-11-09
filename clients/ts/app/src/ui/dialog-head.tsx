// A dialog's title for a Dialog with `padding={0}`: that zero also reaches Astryx's DialogHeader, which
// then sits against the dialog's edge. This one lines up with the 16px of the body below it.

import { DialogHeader, type DialogHeaderProps } from "@astryxdesign/core/Dialog";
import * as stylex from "@stylexjs/stylex";

const styles = stylex.create({
  head: { paddingInline: 16, paddingBlockStart: 16 },
});

export function DialogHead(props: DialogHeaderProps) {
  return <DialogHeader {...props} xstyle={styles.head} />;
}
