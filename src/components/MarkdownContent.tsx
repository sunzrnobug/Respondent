import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

type MarkdownContentProps = {
  children: string;
  className?: string;
};

export function MarkdownContent({
  children,
  className,
}: MarkdownContentProps) {
  const classes = ["mdContent", className].filter(Boolean).join(" ");
  return (
    <div className={classes}>
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{children}</ReactMarkdown>
    </div>
  );
}
