import * as stylex from "@stylexjs/stylex";
import qrcode from "qrcode-generator";
import { useMemo } from "react";

const styles = stylex.create({
  code: { borderRadius: 6, backgroundColor: "#fff" },
});

/** Dark squares on white, whatever the theme: a scanner needs that contrast. */
export function QrCode({ payload, label, size = 148 }: { payload: string; label: string; size?: number }) {
  const path = useMemo(() => {
    const code = qrcode(0, "M");
    code.addData(payload);
    code.make();
    const n = code.getModuleCount();
    let d = "";
    for (let y = 0; y < n; y++) {
      for (let x = 0; x < n; x++) if (code.isDark(y, x)) d += `M${x + 2} ${y + 2}h1v1h-1z`;
    }
    return { d, side: n + 4 };
  }, [payload]);
  return (
    <svg
      role="img"
      aria-label={label}
      width={size}
      height={size}
      viewBox={`0 0 ${path.side} ${path.side}`}
      {...stylex.props(styles.code)}
    >
      <path d={path.d} fill="#000" />
    </svg>
  );
}
