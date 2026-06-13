type ToggleSwitchProps = {
  checked: boolean;
  onChange: (checked: boolean) => void;
  "aria-label": string;
  id?: string;
};

export function ToggleSwitch({
  checked,
  onChange,
  "aria-label": ariaLabel,
  id,
}: ToggleSwitchProps) {
  return (
    <span className="toggleSwitch">
      <input
        id={id}
        className="toggleSwitchInput"
        type="checkbox"
        checked={checked}
        aria-label={ariaLabel}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="toggleSwitchVisual" aria-hidden="true" />
    </span>
  );
}
