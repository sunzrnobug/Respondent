import { useId, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";

type FieldLabelProps = {
  children: string;
  tooltip: string;
  required?: boolean;
};

export function FieldLabel({ children, tooltip, required = false }: FieldLabelProps) {
  const tooltipId = useId();
  const anchorRef = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [tooltipStyle, setTooltipStyle] = useState<CSSProperties>({});

  const showTooltip = () => {
    const anchor = anchorRef.current;
    if (!anchor) return;

    const rect = anchor.getBoundingClientRect();
    const maxWidth = Math.min(280, window.innerWidth - 16);
    const left = Math.min(
      Math.max(8, rect.left),
      window.innerWidth - maxWidth - 8,
    );

    setTooltipStyle({
      position: "fixed",
      top: rect.bottom + 6,
      left,
      width: maxWidth,
      zIndex: 1100,
    });
    setOpen(true);
  };

  const hideTooltip = () => setOpen(false);

  return (
    <>
      <span
        ref={anchorRef}
        className="fieldLabel"
        aria-describedby={open ? tooltipId : undefined}
        onMouseEnter={showTooltip}
        onMouseLeave={hideTooltip}
        onFocus={showTooltip}
        onBlur={hideTooltip}
        tabIndex={0}
      >
        <span className="fieldLabelText">{children}</span>
        {required ? (
          <span className="fieldRequiredMark" aria-hidden="true">
            *
          </span>
        ) : null}
      </span>
      {open
        ? createPortal(
            <span
              id={tooltipId}
              role="tooltip"
              className="fieldTooltip"
              style={tooltipStyle}
            >
              {tooltip}
            </span>,
            document.body,
          )
        : null}
    </>
  );
}
