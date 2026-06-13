import { Check, ChevronDown } from "lucide-react";
import { useEffect, useId, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { resolveProviderLogo } from "../assets/providerLogos";

export type ProviderSelectOption = {
  value: string;
  label: string;
};

type ProviderSelectProps = {
  value: string;
  options: readonly ProviderSelectOption[];
  onChange: (value: string) => void;
  "aria-label": string;
};

function ProviderLogo({
  providerValue,
  className,
}: {
  providerValue: string;
  className?: string;
}) {
  const logoSrc = resolveProviderLogo(providerValue);
  if (!logoSrc) return null;

  return (
    <img
      className={className}
      src={logoSrc}
      alt=""
      aria-hidden="true"
      draggable={false}
    />
  );
}

export function ProviderSelect({
  value,
  options,
  onChange,
  "aria-label": ariaLabel,
}: ProviderSelectProps) {
  const listboxId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLUListElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [menuStyle, setMenuStyle] = useState<CSSProperties>({});
  const selected = options.find((option) => option.value === value);

  const updateMenuPosition = () => {
    const trigger = triggerRef.current;
    if (!trigger) return;

    const rect = trigger.getBoundingClientRect();
    const menuMaxHeight = Math.min(200, window.innerHeight * 0.36);
    const gap = 3;
    const spaceBelow = window.innerHeight - rect.bottom - gap;
    const spaceAbove = rect.top - gap;
    const openUpward = spaceBelow < menuMaxHeight && spaceAbove > spaceBelow;

    setMenuStyle({
      position: "fixed",
      left: rect.left,
      width: rect.width,
      zIndex: 1000,
      ...(openUpward
        ? { bottom: window.innerHeight - rect.top + gap }
        : { top: rect.bottom + gap }),
      maxHeight: openUpward
        ? Math.min(menuMaxHeight, spaceAbove)
        : Math.min(menuMaxHeight, spaceBelow),
    });
  };

  const openMenu = () => {
    updateMenuPosition();
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (rootRef.current?.contains(target)) return;
      if (menuRef.current?.contains(target)) return;
      setOpen(false);
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    };

    const handleDismiss = () => setOpen(false);

    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", handleDismiss);
    window.addEventListener("scroll", handleDismiss, true);

    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", handleDismiss);
      window.removeEventListener("scroll", handleDismiss, true);
    };
  }, [open]);

  return (
    <div
      className={open ? "providerSelectRoot open" : "providerSelectRoot"}
      ref={rootRef}
    >
      <button
        ref={triggerRef}
        type="button"
        className="providerSelectTrigger"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        onClick={() => {
          if (open) {
            setOpen(false);
            return;
          }
          openMenu();
        }}
      >
        <span className="providerSelectTriggerContent">
          {selected ? (
            <ProviderLogo
              providerValue={selected.value}
              className="providerSelectLogo"
            />
          ) : null}
          <span className="providerSelectLabel">
            {selected?.label ?? "请选择"}
          </span>
        </span>
        <ChevronDown size={12} aria-hidden="true" />
      </button>

      {open
        ? createPortal(
            <ul
              ref={menuRef}
              id={listboxId}
              className="providerSelectMenu"
              role="listbox"
              aria-label={ariaLabel}
              style={menuStyle}
            >
              {options.map((option) => {
                const isSelected = option.value === value;
                return (
                  <li key={option.value} role="presentation">
                    <button
                      type="button"
                      role="option"
                      aria-selected={isSelected}
                      className={
                        isSelected
                          ? "providerSelectOption selected"
                          : "providerSelectOption"
                      }
                      onClick={() => {
                        onChange(option.value);
                        setOpen(false);
                        triggerRef.current?.focus();
                      }}
                    >
                      <span className="providerSelectOptionContent">
                        <ProviderLogo
                          providerValue={option.value}
                          className="providerSelectLogo"
                        />
                        <span className="providerSelectLabel">
                          {option.label}
                        </span>
                      </span>
                      {isSelected ? (
                        <Check size={12} aria-hidden="true" />
                      ) : null}
                    </button>
                  </li>
                );
              })}
            </ul>,
            document.body,
          )
        : null}
    </div>
  );
}
