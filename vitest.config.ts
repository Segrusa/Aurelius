import { configDefaults, defineConfig } from 'vitest/config';

// target/ holds build output and the worktrees of other branches (target/wt-*);
// their tests are not this checkout's and fail without their own node_modules.
export default defineConfig({
  test: {
    exclude: [...configDefaults.exclude, 'target/**'],
  },
});
