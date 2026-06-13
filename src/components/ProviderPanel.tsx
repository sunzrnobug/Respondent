import {

  Bot,

  Bookmark,

  KeyRound,

  Mic,

  Save,

  Trash2,

  X,

} from "lucide-react";

import { useMemo } from "react";

import type { ProviderProfileListItem } from "../services/tauriApi";

import {

  isProviderFieldRequired,

  PROVIDER_FIELD_TOOLTIPS,

  validateProviderForm,

} from "../domain/providerFormValidation";

import { FieldLabel } from "./FieldLabel";

import { ProviderSelect } from "./ProviderSelect";
import { ToggleSwitch } from "./ToggleSwitch";



const LLM_PROVIDER_OPTIONS = [

  { value: "openai", label: "OpenAI" },

  { value: "dashscope", label: "DashScope" },

  { value: "zhipu", label: "智谱 / Z.ai" },

  { value: "siliconflow", label: "SiliconFlow" },

  { value: "openai_compatible", label: "OpenAI 兼容" },

] as const;



const ASR_PROVIDER_OPTIONS = [

  { value: "openai_realtime", label: "OpenAI 实时" },

  { value: "bailian_realtime", label: "DashScope 实时" },

  { value: "siliconflow_file", label: "SiliconFlow 文件" },

] as const;



export type ProviderForm = {

  llmProvider: string;

  llmApiKey: string;

  llmBaseUrl: string;

  llmModel: string;

  asrProvider: string;

  asrApiKey: string;

  asrBaseUrl: string;

  asrModel: string;

  asrLanguageHint: string;

  asrMaxSentenceSilenceMs: string;

  asrHeartbeat: boolean;

};



type ProviderPanelProps = {

  providerForm: ProviderForm;

  providerProfiles: ProviderProfileListItem[];

  profileName: string;

  editingProfileId: string | null;

  configStatus: string;

  onProfileNameChange: (value: string) => void;

  onUpdateLlmProvider: (provider: string) => void;

  onUpdateAsrProvider: (provider: string) => void;

  onProviderFormChange: (updater: (current: ProviderForm) => ProviderForm) => void;

  onSave: () => void;

  onActivateProfile: (profileId: string) => void;

  onDeleteProfile: (profileId: string) => void;

  onClose: () => void;

  closeTitle?: string;

  className?: string;

};



function KeyBadge({ configured }: { configured: boolean }) {

  return (

    <span className={configured ? "providerKeyBadge set" : "providerKeyBadge"}>

      <KeyRound size={11} aria-hidden="true" />

      {configured ? "已设置" : "未设置"}

    </span>

  );

}



export function ProviderPanel({

  providerForm,

  providerProfiles,

  profileName,

  editingProfileId,

  configStatus,

  onProfileNameChange,

  onUpdateLlmProvider,

  onUpdateAsrProvider,

  onProviderFormChange,

  onSave,

  onActivateProfile,

  onDeleteProfile,

  onClose,

  closeTitle = "关闭服务商设置",

  className = "modalPanel providerPanel",

}: ProviderPanelProps) {

  const activeProfile = providerProfiles.find((profile) => profile.isActive);

  const activeSummary = providerProfiles.find((profile) => profile.isActive)?.summary;



  const validation = useMemo(

    () =>

      validateProviderForm({

        profileName,

        editingProfileId,

        profiles: providerProfiles,

        form: providerForm,

      }),

    [profileName, editingProfileId, providerProfiles, providerForm],

  );



  const saveDisabledTitle = validation.valid

    ? undefined

    : `请填写必填项：${validation.missingLabels.join("、")}`;



  return (

    <section

      aria-labelledby="providers-title"

      className={className}

      role="dialog"

    >

      <div className="modalHeader">

        <div>

          <h2 id="providers-title">服务商配置</h2>

          <div className="configStatus">

            {activeProfile ? `当前：${activeProfile.name}` : "暂无激活配置"}

          </div>

          <div className="providerHeaderBadges">

            <KeyBadge configured={!!activeSummary?.llm?.hasApiKey} />

            <span className="providerHeaderBadgeDivider">LLM</span>

            <KeyBadge configured={!!activeSummary?.asr?.hasApiKey} />

            <span className="providerHeaderBadgeDivider">ASR</span>

          </div>

        </div>

        <button type="button" onClick={onClose} title={closeTitle}>

          <X size={16} />

        </button>

      </div>



      <div className="providerPanelBody">

        <section className="providerSection" aria-labelledby="llm-section-title">

          <div className="providerSectionHeader">

            <div className="providerSectionIcon" aria-hidden="true">

              <Bot size={16} />

            </div>

            <div>

              <h3 id="llm-section-title">LLM 大模型</h3>

              <p>用于生成建议回复</p>

            </div>

          </div>

          <div className="configGrid">

            <label>

              <FieldLabel

                tooltip={PROVIDER_FIELD_TOOLTIPS.llmProvider}

                required={isProviderFieldRequired("llmProvider", providerForm)}

              >

                服务商

              </FieldLabel>

              <ProviderSelect

                aria-label="LLM 服务商"

                value={providerForm.llmProvider}

                options={LLM_PROVIDER_OPTIONS}

                onChange={onUpdateLlmProvider}

              />

            </label>

            <label>

              <FieldLabel

                tooltip={PROVIDER_FIELD_TOOLTIPS.llmApiKey}

                required={isProviderFieldRequired("llmApiKey", providerForm)}

              >

                API 密钥

              </FieldLabel>

              <input

                aria-label="LLM API 密钥"

                aria-required="true"

                type="password"

                placeholder="sk-..."

                value={providerForm.llmApiKey}

                onChange={(event) =>

                  onProviderFormChange((current) => ({

                    ...current,

                    llmApiKey: event.target.value,

                  }))

                }

              />

            </label>

            <label>

              <FieldLabel

                tooltip={PROVIDER_FIELD_TOOLTIPS.llmBaseUrl}

                required={isProviderFieldRequired("llmBaseUrl", providerForm)}

              >

                接口地址

              </FieldLabel>

              <input

                aria-label="LLM 接口地址"

                aria-required={isProviderFieldRequired("llmBaseUrl", providerForm)}

                placeholder="https://..."

                value={providerForm.llmBaseUrl}

                onChange={(event) =>

                  onProviderFormChange((current) => ({

                    ...current,

                    llmBaseUrl: event.target.value,

                  }))

                }

              />

            </label>

            <label>

              <FieldLabel

                tooltip={PROVIDER_FIELD_TOOLTIPS.llmModel}

                required={isProviderFieldRequired("llmModel", providerForm)}

              >

                模型

              </FieldLabel>

              <input

                aria-label="LLM 模型"

                aria-required="true"

                placeholder="gpt-4o-mini"

                value={providerForm.llmModel}

                onChange={(event) =>

                  onProviderFormChange((current) => ({

                    ...current,

                    llmModel: event.target.value,

                  }))

                }

              />

            </label>

          </div>

        </section>



        <section className="providerSection" aria-labelledby="asr-section-title">

          <div className="providerSectionHeader">

            <div className="providerSectionIcon asr" aria-hidden="true">

              <Mic size={16} />

            </div>

            <div>

              <h3 id="asr-section-title">ASR 语音识别</h3>

              <p>用于实时转写麦克风输入</p>

            </div>

          </div>

          <div className="configGrid">

            <label>

              <FieldLabel

                tooltip={PROVIDER_FIELD_TOOLTIPS.asrProvider}

                required={isProviderFieldRequired("asrProvider", providerForm)}

              >

                服务商

              </FieldLabel>

              <ProviderSelect

                aria-label="ASR 服务商"

                value={providerForm.asrProvider}

                options={ASR_PROVIDER_OPTIONS}

                onChange={onUpdateAsrProvider}

              />

            </label>

            <label>

              <FieldLabel

                tooltip={PROVIDER_FIELD_TOOLTIPS.asrApiKey}

                required={isProviderFieldRequired("asrApiKey", providerForm)}

              >

                API 密钥

              </FieldLabel>

              <input

                aria-label="ASR API 密钥"

                aria-required="true"

                type="password"

                placeholder="sk-..."

                value={providerForm.asrApiKey}

                onChange={(event) =>

                  onProviderFormChange((current) => ({

                    ...current,

                    asrApiKey: event.target.value,

                  }))

                }

              />

            </label>

            <label>

              <FieldLabel

                tooltip={PROVIDER_FIELD_TOOLTIPS.asrBaseUrl}

                required={isProviderFieldRequired("asrBaseUrl", providerForm)}

              >

                接口地址

              </FieldLabel>

              <input

                aria-label="ASR 接口地址"

                placeholder="wss://..."

                value={providerForm.asrBaseUrl}

                onChange={(event) =>

                  onProviderFormChange((current) => ({

                    ...current,

                    asrBaseUrl: event.target.value,

                  }))

                }

              />

            </label>

            <label>

              <FieldLabel

                tooltip={PROVIDER_FIELD_TOOLTIPS.asrModel}

                required={isProviderFieldRequired("asrModel", providerForm)}

              >

                模型

              </FieldLabel>

              <input

                aria-label="ASR 模型"

                aria-required="true"

                value={providerForm.asrModel}

                onChange={(event) =>

                  onProviderFormChange((current) => ({

                    ...current,

                    asrModel: event.target.value,

                  }))

                }

              />

            </label>

            {providerForm.asrProvider === "bailian_realtime" ? (

              <>

                <label>

                  <FieldLabel tooltip={PROVIDER_FIELD_TOOLTIPS.asrLanguageHint}>

                    语言提示

                  </FieldLabel>

                  <input

                    aria-label="语言提示"

                    placeholder="zh"

                    value={providerForm.asrLanguageHint}

                    onChange={(event) =>

                      onProviderFormChange((current) => ({

                        ...current,

                        asrLanguageHint: event.target.value,

                      }))

                    }

                  />

                </label>

                <label>

                  <FieldLabel

                    tooltip={PROVIDER_FIELD_TOOLTIPS.asrMaxSentenceSilenceMs}

                  >

                    静音时长（毫秒）

                  </FieldLabel>

                  <input

                    aria-label="静音时长（毫秒）"

                    inputMode="numeric"

                    value={providerForm.asrMaxSentenceSilenceMs}

                    onChange={(event) =>

                      onProviderFormChange((current) => ({

                        ...current,

                        asrMaxSentenceSilenceMs: event.target.value,

                      }))

                    }

                  />

                </label>

                <label className="toggleSwitchField">
                  <span className="toggleSwitchFieldLabel">
                    <FieldLabel tooltip={PROVIDER_FIELD_TOOLTIPS.asrHeartbeat}>
                      心跳保活
                    </FieldLabel>
                  </span>
                  <ToggleSwitch
                    aria-label="心跳保活"
                    checked={providerForm.asrHeartbeat}
                    onChange={(checked) =>
                      onProviderFormChange((current) => ({
                        ...current,
                        asrHeartbeat: checked,
                      }))
                    }
                  />
                </label>

              </>

            ) : null}

          </div>

        </section>



        <section

          className="providerSection providerProfilesSection"

          aria-label="已保存的服务商配置"

        >

          <div className="providerSectionHeader compact">

            <div className="providerSectionIcon saved" aria-hidden="true">

              <Bookmark size={16} />

            </div>

            <div>

              <h3>已保存的配置</h3>

              <p>{providerProfiles.length} 套配置方案</p>

            </div>

          </div>

          {providerProfiles.length === 0 ? (

            <div className="providerProfilesEmpty">

              <Bookmark size={24} strokeWidth={1.4} aria-hidden="true" />

              <strong>还没有保存的配置</strong>

              <p>填写上方表单并命名后，可保存多套服务商方案并快速切换。</p>

            </div>

          ) : (

            <ul className="providerProfilesList">

              {providerProfiles.map((profile) => (

                <li className="providerProfileItem" key={profile.id}>

                  <button

                    className={

                      profile.isActive

                        ? "providerProfileButton selected"

                        : "providerProfileButton"

                    }

                    type="button"

                    onClick={() => onActivateProfile(profile.id)}

                  >

                    <span className="providerProfileName">{profile.name}</span>

                    <span className="providerProfileMeta">

                      {profile.summary.llm?.provider ?? "无 LLM"} ·{" "}

                      {profile.summary.asr?.provider ?? "无 ASR"}

                    </span>

                  </button>

                  <button

                    className="providerProfileDelete"

                    type="button"

                    title={`删除 ${profile.name}`}

                    onClick={() => onDeleteProfile(profile.id)}

                  >

                    <Trash2 size={15} />

                  </button>

                </li>

              ))}

            </ul>

          )}

        </section>

      </div>



      <div className="modalFooter providerPanelFooter">

        <label className="profileNameField">

          <FieldLabel

            tooltip={PROVIDER_FIELD_TOOLTIPS.profileName}

            required={isProviderFieldRequired("profileName", providerForm)}

          >

            配置名称

          </FieldLabel>

          <input

            aria-label="配置名称"

            aria-required="true"

            placeholder="为这套服务商配置命名"

            value={profileName}

            onChange={(event) => onProfileNameChange(event.target.value)}

          />

        </label>

        <div className="providerFooterActions">

          {configStatus ? (

            <div className="configStatus">{configStatus}</div>

          ) : (

            <div />

          )}

          <button

            className="primaryButton"

            type="button"

            disabled={!validation.valid}

            title={saveDisabledTitle}

            onClick={onSave}

          >

            <Save size={15} aria-hidden="true" />

            {editingProfileId ? "保存修改" : "保存配置"}

          </button>

        </div>

      </div>

    </section>

  );

}


