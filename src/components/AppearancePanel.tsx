import { Moon, Sun, X } from "lucide-react";
import type { AppearanceTheme } from "../state/appearanceSettings";
type AppearancePanelProps = {
  windowOpacity: number;
  windowBlur: number;
  appearanceTheme: AppearanceTheme;
  onWindowOpacityChange: (value: number) => void;
  onWindowBlurChange: (value: number) => void;
  onAppearanceThemeChange: (theme: AppearanceTheme) => void;
  onClose: () => void;
  closeTitle?: string;
  className?: string;
};

export function AppearancePanel({
  windowOpacity,
  windowBlur,
  appearanceTheme,
  onWindowOpacityChange,
  onWindowBlurChange,
  onAppearanceThemeChange,
  onClose,
  closeTitle = "关闭外观设置",
  className = "modalPanel appearancePanel",
}: AppearancePanelProps) {
  return (    <section
      aria-labelledby="appearance-title"
      className={className}
      role="dialog"
    >
      <div className="modalHeader">
        <div>
          <h2 id="appearance-title">外观</h2>
          <div className="configStatus">
            玻璃界面 · 透明度 {windowOpacity}% · 模糊 {windowBlur}px
          </div>
        </div>
        <button type="button" onClick={onClose} title={closeTitle}>
          <X size={16} />
        </button>
      </div>

      <div className="appearancePanelBody">
        <div className="appearanceControls">          <label className="appearanceRangeCard">
            <span className="appearanceRangeLabel">
              窗口透明度
              <strong>{windowOpacity}%</strong>
            </span>
            <input
              aria-label="窗口透明度"
              type="range"
              min="55"
              max="92"
              value={windowOpacity}
              onChange={(event) =>
                onWindowOpacityChange(Number(event.target.value))
              }
            />
          </label>
          <label className="appearanceRangeCard">
            <span className="appearanceRangeLabel">
              背景模糊
              <strong>{windowBlur}px</strong>
            </span>
            <input
              aria-label="背景模糊"
              type="range"
              min="8"
              max="32"
              value={windowBlur}
              onChange={(event) =>
                onWindowBlurChange(Number(event.target.value))
              }
            />
          </label>
        </div>

        <div className="appearanceSection">
          <div className="appearanceSectionTitle">主题风格</div>
          <div className="themeSwitch" aria-label="外观主题">
            <button
              className={
                appearanceTheme === "dark"
                  ? "themeSwitchOption selected"
                  : "themeSwitchOption"
              }
              type="button"
              onClick={() => onAppearanceThemeChange("dark")}
            >
              <Moon size={15} aria-hidden="true" />
              <span>深色玻璃</span>
            </button>
            <button
              className={
                appearanceTheme === "light"
                  ? "themeSwitchOption selected"
                  : "themeSwitchOption"
              }
              type="button"
              onClick={() => onAppearanceThemeChange("light")}
            >
              <Sun size={15} aria-hidden="true" />
              <span>浅色玻璃</span>
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}
