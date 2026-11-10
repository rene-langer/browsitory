// Generated from the backend's limits and defaults (crates/repo-service/src/wire.rs).
// Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p repo-service wire_bindings_are_current
import type { AppearanceSettings, CommitMessageSettings, DiffSettings, FetchSettings } from "./wire-dto";

/** `git_core::diff::MAX_CONTEXT_LINES`: more context is clamped to it. */
export const MAX_CONTEXT_LINES = 20;
/** `git_core::diff::WHOLE_FILE_CONTEXT_LINES`: the context that shows the whole file. */
export const WHOLE_FILE_CONTEXT_LINES = 4294967295;
/** `config::COMMIT_MESSAGE_COLUMNS`: the range commit message columns are clamped to. */
export const COMMIT_MESSAGE_COLUMNS = { min: 10, max: 200 } as const;
/** `config::DiffSettings::default()`. */
export const DEFAULT_DIFF_SETTINGS: DiffSettings = {"ignoreWhitespace":false,"contextLines":3,"syntaxHighlight":true,"wordWrap":"Auto","algorithm":"Default","showWhitespace":false,"expandUntracked":true};
/** `config::CommitMessageSettings::default()`. */
export const DEFAULT_COMMIT_MESSAGE_SETTINGS: CommitMessageSettings = {"summaryLimit":50,"bodyRuler":72,"showRuler":true};
/** `config::FetchSettings::default()`. */
export const DEFAULT_FETCH_SETTINGS: FetchSettings = {"autoFetchMinutes":0};
/** `config::AppearanceSettings::default()`. */
export const DEFAULT_APPEARANCE_SETTINGS: AppearanceSettings = {"codeFont":null,"codeFontSize":null,"uiSize":"Normal","tabSize":4};
/** `config::CODE_FONT_SIZES`: the range the code font size is clamped to, in px. */
export const CODE_FONT_SIZES = { min: 10, max: 24 } as const;
/** `config::TAB_SIZES`: the range the tab size is clamped to, in columns. */
export const TAB_SIZES = { min: 1, max: 16 } as const;
/** `git_core::search::KEYS`: the keys a commit search `key:value` term can use. */
export const SEARCH_KEYS = ["author","committer","message","commit","path","contents","after","before","branch","line","merges"] as const;
