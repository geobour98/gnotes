// Type declarations for markdown-it plugins that ship without types.
declare module "markdown-it-texmath" {
  import type { MarkdownIt } from "markdown-it";
  const texmath: (md: MarkdownIt, options?: Record<string, unknown>) => void;
  export default texmath;
}
declare module "markdown-it-mark" {
  import type { MarkdownIt } from "markdown-it";
  const mark: (md: MarkdownIt) => void;
  export default mark;
}
declare module "markdown-it-footnote" {
  import type { MarkdownIt } from "markdown-it";
  const footnote: (md: MarkdownIt) => void;
  export default footnote;
}
