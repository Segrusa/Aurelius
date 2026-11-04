// Radix's scroll lock writes <style> elements, which the page's CSP refuses, and no screen needs
// them: the page itself never scrolls behind a dialog. The bundlers point `react-style-singleton`
// here, so nothing is attempted and blocked.

const nothing = () => null;

export const styleSingleton = () => nothing;
export const styleHookSingleton = () => () => undefined;
export const stylesheetSingleton = () => ({ add: () => undefined, remove: () => undefined });
