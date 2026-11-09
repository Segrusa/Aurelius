// Someone is writing: three dots, as in any chat.

import * as stylex from "@stylexjs/stylex";

const bob = stylex.keyframes({
  "0%, 60%, 100%": { opacity: 0.3, transform: "translateY(0)" },
  "30%": { opacity: 1, transform: "translateY(-2px)" },
});

const styles = stylex.create({
  dots: { display: "inline-flex", alignItems: "center", gap: 3, height: 12 },
  dot: {
    width: 5,
    height: 5,
    borderRadius: "50%",
    backgroundColor: "var(--color-text-secondary)",
    animationName: bob,
    animationDuration: "1.1s",
    animationIterationCount: "infinite",
  },
  second: { animationDelay: "0.15s" },
  third: { animationDelay: "0.3s" },
});

export function TypingDots() {
  return (
    <span aria-hidden="true" {...stylex.props(styles.dots)}>
      <span {...stylex.props(styles.dot)} />
      <span {...stylex.props(styles.dot, styles.second)} />
      <span {...stylex.props(styles.dot, styles.third)} />
    </span>
  );
}
