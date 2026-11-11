import { USER_GUIDE_URL } from "./lib/links";
import { ariaKeyShortcuts, shortcutLabel } from "./lib/keybindings";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Moon, Settings, Sparkles, Sun } from "lucide-react";
import { CommandsButton } from "./components/CommandsButton";
import { HelpMenuButton } from "./components/HelpMenuButton";
import { RepoPicker } from "./components/RepoPicker";
import { RepoTabs } from "./components/RepoTabs";
import { RenameTabDialog } from "./components/RenameTabDialog";
import { ReleaseNotesModal } from "./components/ReleaseNotesModal";
import { ThirdPartyLicencesDialog } from "./components/ThirdPartyLicencesDialog";
import { ToastRegion } from "./components/primitives/ToastRegion";
import { useToasts } from "./state/useToasts";
import { useFileDrops } from "./state/useFileDrops";
import { requestWorkspace } from "./lib/workspaceRequests";
import { InlineError } from "./components/primitives/InlineError";
import { ConfirmDialog } from "./components/primitives/ConfirmDialog";
import { CloneDialog } from "./components/CloneDialog";
import { Overlay } from "./components/primitives/Overlay";
import type { RepoClient, RootFileName } from "./ipc/RepoClient";
import { openRootFile, rootFileItems } from "./lib/rootFiles";
import { subscribeTransportStatus } from "./ipc/transportStatus";
import type { AppCommands } from "./lib/commands";
import {
  AppearanceSection,
  CommitMessageSection,
  DiffSection,
  ExternalToolsSection,
  FetchSection,
  PreferencesDialog,
  UpdatesSection,
} from "./components/PreferencesDialog";
import { cloneToastMessage, createNewRepository } from "./lib/newRepository";
import { repoNameOf } from "./lib/repoName";
import { AppearanceSettingsProvider } from "./state/AppearanceSettingsProvider";
import { CommitMessageSettingsProvider } from "./state/CommitMessageSettingsProvider";
import { FetchSettingsProvider } from "./state/FetchSettingsProvider";
import { UpdateSettingsProvider } from "./state/UpdateSettingsProvider";
import {
  UpdateCheckContext,
  type UpdateCheck,
  type UpdateCheckOutcome,
  type UpdateCheckRegistry,
} from "./state/updateCheck";
import { DiffSettingsProvider } from "./state/DiffSettingsProvider";
import { useOpenRepos } from "./state/useOpenRepos";
import { useTabChangeMarkers } from "./state/useTabChangeMarkers";
import { useWorkspaces } from "./state/useWorkspaces";
import styles from "./App.module.css";
import { RepoWorkspace } from "./RepoWorkspace";
import { useBusyPaths } from "./state/useBusyPaths";
import { useCommitDrafts } from "./state/useCommitDrafts";
import { useE2eAutoOpen } from "./state/useE2eAutoOpen";
import { useHostRequests } from "./state/useHostRequests";
import { allReleaseNotes, useReleaseNotes } from "./state/useReleaseNotes";
import { useTheme } from "./state/useTheme";

export default function App(props: {
  client: RepoClient;
  /**
   * The desktop app's `UpdateBanner`. Given, the app also offers the "Updates" preferences and
   * "Check for updates" (#630); the VS Code extension updates through its marketplace and has none.
   */
  updateBanner?: ReactNode;
  /** Which host launches external tools; VSCode opens them in VSCode itself. */
  toolHost?: "desktop" | "vscode";
  /** Loads THIRD_PARTY_NOTICES.md (`lib/thirdPartyNotices.ts`); without it the app offers no licences view. */
  loadThirdPartyNotices?: () => Promise<string>;
}) {
  return (
    <DiffSettingsProvider client={props.client}>
      <CommitMessageSettingsProvider client={props.client}>
        <FetchSettingsProvider client={props.client}>
          <AppearanceSettingsProvider client={props.client}>
            <UpdateSettingsProvider client={props.client} enabled={hasUpdater(props.updateBanner)}>
              <AppContent {...props} />
            </UpdateSettingsProvider>
          </AppearanceSettingsProvider>
        </FetchSettingsProvider>
      </CommitMessageSettingsProvider>
    </DiffSettingsProvider>
  );
}

const hasUpdater = (updateBanner: ReactNode) => updateBanner !== undefined && updateBanner !== null;

function AppContent({
  client,
  updateBanner = null,
  toolHost = "desktop",
  loadThirdPartyNotices,
}: {
  client: RepoClient;
  updateBanner?: ReactNode;
  toolHost?: "desktop" | "vscode";
  loadThirdPartyNotices?: () => Promise<string>;
}) {
  const [preferencesOpen, setPreferencesOpen] = useState(false);
  const [licencesOpen, setLicencesOpen] = useState(false);
  // Failures of the repo tab menu's actions (terminal, reveal); workspace tool errors show in
  // that workspace.
  const [tabToolError, setTabToolError] = useState<string | null>(null);
  const openRepos = useOpenRepos(client);
  const tabChangeMarkers = useTabChangeMarkers(client, openRepos.openRepos, openRepos.activePath);
  const workspaces = useWorkspaces(client);
  const workspaceNames = useMemo(
    () => Object.fromEntries(workspaces.workspaces.map((workspace) => [workspace.id, workspace.name])),
    [workspaces.workspaces],
  );
  // In VSCode the editor's theme decides (#676): no toggle, no setting, nothing applied.
  const themeControls = toolHost !== "vscode";
  const { preference: themePreference, theme, setPreference: setThemePreference } = useTheme(themeControls);
  const { onBusyChange, busyPaths } = useBusyPaths();
  const { getCommitDraft, onCommitDraftChange } = useCommitDrafts();

  const [pickingRepo, setPickingRepo] = useState(false);
  // The tab whose "Rename tab…" dialog is open (#625).
  const [renamingTab, setRenamingTab] = useState<string | null>(null);
  const { activePath } = openRepos;
  const renameActiveTab = useCallback(() => setRenamingTab(activePath), [activePath]);

  // `RepoPicker` only surfaces errors from its own `pickRepoFolder`/`listRecentRepos` calls, and
  // `useOpenRepos.openRepo` deliberately rejects rather than opening a tab for a repo that failed
  // to open (bad path, a stale recent-repo entry, permissions). Nothing else catches that now
  // that `App` has no `useAppState` of its own, so a failed open would look like nothing
  // happened — the same trap the pre-tabs `App` carried a comment about.
  const [openError, setOpenError] = useState<{ message: string; path: string } | null>(null);
  const [transportError, setTransportError] = useState<string | null>(null);
  useEffect(
    () => subscribeTransportStatus((status) => setTransportError(status.message)),
    [],
  );
  const { openRepo } = openRepos;
  const openRepoTab = useCallback(
    // Both branches settle asynchronously on purpose: the E2E auto-open effect (`useE2eAutoOpen`)
    // calls this directly, and a synchronous `setOpenError` in the body would be a setState-in-effect.
    (path: string) => {
      return openRepo(path).then(
        () => setOpenError(null),
        (error: unknown) => setOpenError({ message: String(error), path }),
      );
    },
    [openRepo],
  );

  // "Browsitory: Open", its menus and the contributed shortcut keys in VSCode (#677).
  useHostRequests(client, !openRepos.loading, openRepoTab);

  const appToasts = useToasts();
  const [cloning, setCloning] = useState(false);
  const cloneRunning = useRef(false);
  const [existingRepoPrompt, setExistingRepoPrompt] = useState<string | null>(null);
  const { push: pushAppToast } = appToasts;
  const startClone = useCallback(() => {
    setPickingRepo(false);
    setCloning(true);
  }, []);
  const startNewRepository = useCallback(() => {
    setPickingRepo(false);
    createNewRepository(client).then(
      (result) => {
        if (result.kind === "created") {
          void openRepoTab(result.path);
          pushAppToast(`Created repository ${repoNameOf(result.path)}`);
        } else if (result.kind === "exists") {
          setExistingRepoPrompt(result.path);
        }
      },
      (error: unknown) => setOpenError({ message: String(error instanceof Error ? error.message : error), path: "" }),
    );
  }, [client, openRepoTab, pushAppToast]);
  const openPreferences = useCallback(() => setPreferencesOpen(true), []);
  // "Check for updates" (#630) runs the check the update banner registers.
  const updateCheck = useRef<UpdateCheck | null>(null);
  const updateCheckRegistry = useMemo<UpdateCheckRegistry>(
    () => ({
      register(check) {
        updateCheck.current = check;
        return () => {
          if (updateCheck.current === check) updateCheck.current = null;
        };
      },
    }),
    [],
  );
  const runUpdateCheck = useCallback(
    (): Promise<UpdateCheckOutcome> =>
      updateCheck.current?.() ?? Promise.resolve({ kind: "failed", message: "The updater isn't running." }),
    [],
  );
  const updaterAvailable = hasUpdater(updateBanner);
  const checkForUpdates = useMemo(
    () =>
      updaterAvailable
        ? () => {
            pushAppToast("Checking for updates…", { tone: "info" });
            void runUpdateCheck().then((outcome) => {
              if (outcome.kind === "none") pushAppToast("Browsitory is up to date", { tone: "info" });
              else if (outcome.kind === "ready") pushAppToast(`Update v${outcome.version} is ready: restart to update`, { tone: "info" });
              else setTabToolError(`Couldn't check for updates: ${outcome.message}`);
            });
          }
        : undefined,
    [updaterAvailable, pushAppToast, runUpdateCheck],
  );
  const openLicences = useMemo(
    () => (loadThirdPartyNotices === undefined ? undefined : () => setLicencesOpen(true)),
    [loadThirdPartyNotices],
  );
  // Opens in the browser (the VSCode extension too), never in this window. Failures show in the
  // same app-level banner as the tab menu's tools.
  const openUserGuide = useCallback(() => {
    setTabToolError(null);
    client
      .openExternalUrl(USER_GUIDE_URL)
      .catch((error: unknown) =>
        setTabToolError(`Couldn't open the user guide: ${error instanceof Error ? error.message : String(error)}`),
      );
  }, [client]);
  // "Reopen closed tab" (#613): a closed repository that no longer opens is skipped and reported
  // in the open-error banner, like a failed restore.
  const { reopenClosedRepo, canReopenClosedRepo } = openRepos;
  const reopenClosedTab = useCallback(() => {
    void reopenClosedRepo().then((failures) => {
      if (failures.length === 0) return;
      setOpenError({
        message: failures.map((failure) => `Couldn't reopen ${repoNameOf(failure.path)}: ${failure.message}`).join("\n"),
        path: "",
      });
    });
  }, [reopenClosedRepo]);
  const appCommands = useMemo<AppCommands>(
    () => ({
      ...(client.supportsCloneAndInit ? { onCloneRepository: startClone, onNewRepository: startNewRepository } : {}),
      onOpenPreferences: openPreferences,
      onOpenThirdPartyLicences: openLicences,
      onOpenUserGuide: openUserGuide,
      ...(canReopenClosedRepo ? { onReopenClosedTab: reopenClosedTab } : {}),
      onRenameTab: renameActiveTab,
      onCheckForUpdates: checkForUpdates,
    }),
    [
      client.supportsCloneAndInit,
      startClone,
      startNewRepository,
      openPreferences,
      openLicences,
      openUserGuide,
      canReopenClosedRepo,
      reopenClosedTab,
      renameActiveTab,
      checkForUpdates,
    ],
  );
  // Which of `.gitignore`/`.gitattributes` each open tab's work tree has (#624), for the tab
  // menu's "Open …"/"Create …" items: read as tabs open and again whenever a tab menu opens.
  // `null`: bare, no items.
  const [rootFiles, setRootFiles] = useState<Record<string, RootFileName[] | null>>({});
  const loadRootFiles = useCallback(
    (path: string) => {
      client.getRootFiles(path).then(
        (files) => setRootFiles((prev) => ({ ...prev, [path]: files })),
        // Unknown: the items stay as last read (absent if never read).
        () => {},
      );
    },
    [client],
  );
  const openPathsKey = openRepos.openRepos.map((repo) => repo.path).join("\0");
  useEffect(() => {
    if (openPathsKey !== "") openPathsKey.split("\0").forEach((path) => loadRootFiles(path));
  }, [openPathsKey, loadRootFiles]);
  const tabMenuItems = useCallback(
    (path: string) => {
      const run = (action: () => Promise<void>) => {
        setTabToolError(null);
        action().catch((error: unknown) => setTabToolError(error instanceof Error ? error.message : String(error)));
      };
      // Grouped (#645): the tab, the folder's tools, its root files, then reopening.
      return [
        { label: "Rename tab…", onSelect: () => setRenamingTab(path), shortcut: "F2", ariaKeyShortcuts: "F2" },
        { label: "Open terminal here", onSelect: () => run(() => client.openTerminal(path)), group: "tools" },
        { label: "Reveal repository", onSelect: () => run(() => client.revealInFileManager(path, null)), group: "tools" },
        ...rootFileItems(rootFiles[path]).map((item) => ({
          group: "files",
          label: item.label,
          onSelect: () =>
            run(async () => {
              try {
                await openRootFile(client, path, item);
              } finally {
                if (!item.exists) loadRootFiles(path);
              }
            }),
        })),
        {
          label: "Reopen closed tab",
          onSelect: reopenClosedTab,
          disabled: !canReopenClosedRepo,
          title: canReopenClosedRepo ? undefined : "No closed tab to reopen",
          group: "reopen",
          shortcut: shortcutLabel("reopen-closed-tab"),
          ariaKeyShortcuts: ariaKeyShortcuts("reopen-closed-tab"),
        },
      ];
    },
    [client, reopenClosedTab, canReopenClosedRepo, rootFiles, loadRootFiles],
  );

  const { releaseNotesView, setReleaseNotesView, closeReleaseNotes } = useReleaseNotes(client);

  // Switches to the other theme; Preferences > Appearance > Theme goes back to System (#691).
  const themeToggle = themeControls && (
    <button
      type="button"
      className={styles.iconButton}
      aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
      onClick={() => setThemePreference(theme === "dark" ? "light" : "dark")}
    >
      {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
    </button>
  );

  useE2eAutoOpen(openRepos, openRepoTab);

  // Folders dropped on the window open as tabs, one after the other; patch files are applied to
  // the active tab's working tree (#649).
  const fileDragging = useFileDrops(client, ({ folders, patches }) => {
    void folders.reduce<Promise<unknown>>((previous, folder) => previous.then(() => openRepoTab(folder)), Promise.resolve());
    if (patches.length === 0) return;
    if (activePath === null) {
      const names = patches.map((path) => path.split(/[\\/]/).pop() ?? path).join(", ");
      setOpenError({ message: `Open a repository to apply ${names} to.`, path: "" });
    } else {
      requestWorkspace({ kind: "applyPatches", paths: patches });
    }
  });
  const dropHint = fileDragging && (
    <div className={styles.dropHint} aria-hidden="true">
      <p>Drop a folder to open it, or a patch file to apply it to the current repository</p>
    </div>
  );

  if (openRepos.loading) {
    return (
      <main>
        <header className={styles.headerRow}>
          <h1>Browsitory</h1>
          <div className={styles.headerSpacer} />
          {themeToggle}
        </header>
        <p role="status" className={styles.loading}>
          Loading…
        </p>
      </main>
    );
  }

  return (
    <main>
      {dropHint}
      <UpdateCheckContext.Provider value={updateCheckRegistry}>{updateBanner}</UpdateCheckContext.Provider>
      <header className={styles.headerRow}>
        <h1>Browsitory</h1>
        <RepoTabs
          openRepos={openRepos.openRepos}
          activePath={openRepos.activePath}
          busyPaths={busyPaths}
          workspaceNames={workspaceNames}
          onSwitchTo={openRepos.switchTo}
          onClose={openRepos.closeRepo}
          onCloseGroup={(paths) => paths.forEach((path) => openRepos.closeRepo(path))}
          onAddTab={() => setPickingRepo(true)}
          tabMenuItems={tabMenuItems}
          onTabMenuOpen={loadRootFiles}
          changedPaths={tabChangeMarkers.changedPaths}
          onRenameTab={setRenamingTab}
        />
        {openRepos.openRepos.length === 0 && <div className={styles.headerSpacer} />}
        {openRepos.openRepos.length > 0 && <CommandsButton />}
        {themeToggle}
        <button
          type="button"
          className={styles.iconButton}
          aria-label="Preferences"
          title="Preferences"
          onClick={openPreferences}
        >
          <Settings size={16} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={styles.iconButton}
          aria-label="Release notes"
          title="What’s new"
          onClick={() => setReleaseNotesView({ mode: "all", entries: allReleaseNotes })}
        >
          <Sparkles size={16} aria-hidden="true" />
        </button>
        <HelpMenuButton
          className={styles.iconButton}
          shortcuts={openRepos.openRepos.length > 0}
          onOpenUserGuide={openUserGuide}
          onShowReleaseNotes={() => setReleaseNotesView({ mode: "all", entries: allReleaseNotes })}
        />
      </header>
      {openRepos.restoreError !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={openRepos.restoreError} copyDetails onDismiss={openRepos.dismissRestoreError} />
        </div>
      )}
      {tabToolError !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={tabToolError} copyDetails onDismiss={() => setTabToolError(null)} />
        </div>
      )}
      {preferencesOpen && (
        <PreferencesDialog onClose={() => setPreferencesOpen(false)}>
          <ExternalToolsSection client={client} repoPath={openRepos.activePath} toolHost={toolHost} />
          <CommitMessageSection />
          <FetchSection repoPath={openRepos.activePath} />
          <DiffSection />
          <AppearanceSection
            theme={themeControls ? { preference: themePreference, onChange: setThemePreference } : undefined}
          />
          {updaterAvailable && <UpdatesSection onCheckNow={runUpdateCheck} />}
        </PreferencesDialog>
      )}
      {(transportError !== null || openError !== null) && (
        <div className={styles.errorLayer}>
          {transportError !== null && (
            <InlineError
              message={transportError}
              hint="Browsitory lost its connection to the backend. Restart the app if this keeps happening."
              copyDetails
              onDismiss={() => setTransportError(null)}
            />
          )}
          {openError !== null && (
            <InlineError
              message={openError.message}
              hint={openError.path !== "" ? "Check that the folder exists and is a Git repository." : undefined}
              onRetry={openError.path !== "" ? () => void openRepoTab(openError.path) : undefined}
              copyDetails
              onDismiss={() => setOpenError(null)}
            />
          )}
        </div>
      )}
      {releaseNotesView !== null && (
        <ReleaseNotesModal
          entries={releaseNotesView.entries}
          onClose={closeReleaseNotes}
          // Close the dialog first: a failure shows in the app-level banner, which the overlay
          // would cover.
          onOpenUserGuide={() => {
            closeReleaseNotes();
            openUserGuide();
          }}
          onOpenThirdPartyLicences={
            openLicences === undefined
              ? undefined
              : () => {
                  closeReleaseNotes();
                  openLicences();
                }
          }
        />
      )}
      {licencesOpen && loadThirdPartyNotices !== undefined && (
        <ThirdPartyLicencesDialog load={loadThirdPartyNotices} onClose={() => setLicencesOpen(false)} />
      )}
      {renamingTab !== null && (
        <RenameTabDialog
          key={renamingTab}
          path={renamingTab}
          alias={openRepos.aliases.get(renamingTab) ?? null}
          onRename={(alias) => openRepos.renameRepo(renamingTab, alias).then(() => setRenamingTab(null))}
          onCancel={() => setRenamingTab(null)}
        />
      )}
      {pickingRepo && (
        <Overlay onClose={() => setPickingRepo(false)} ariaLabel="Open repository">
          <RepoPicker
            client={client}
            onOpenRepo={(path) => {
              void openRepoTab(path);
              setPickingRepo(false);
            }}
            onOpenWorkspace={(workspace) => {
              void openRepos.openWorkspace(workspace);
              setPickingRepo(false);
            }}
            workspaces={workspaces.workspaces}
            aliases={openRepos.aliases}
            workspacesLoading={workspaces.loading}
            workspacesError={workspaces.error}
            onDismissWorkspacesError={workspaces.dismissError}
            onCreateWorkspace={workspaces.createWorkspace}
            onEditWorkspace={workspaces.editWorkspace}
            onDeleteWorkspace={workspaces.deleteWorkspace}
            onCloneRepository={appCommands?.onCloneRepository}
            onNewRepository={appCommands?.onNewRepository}
          />
        </Overlay>
      )}
      {cloning && (
        // FormDialog already blocks Escape while running; this guard is belt-and-braces for the
        // backdrop path so a running clone is never dismissed invisibly.
        <Overlay
          ariaLabel="Clone repository"
          onClose={() => {
            if (!cloneRunning.current) setCloning(false);
          }}
        >
          <CloneDialog
            client={client}
            onCancel={() => setCloning(false)}
            onRunningChange={(running) => (cloneRunning.current = running)}
            onCloned={(outcome, name) => {
              setCloning(false);
              void openRepoTab(outcome.path);
              const incomplete = outcome.submoduleFailures.length > 0 || outcome.credentialSaveFailed;
              pushAppToast(cloneToastMessage(outcome, name), { tone: incomplete ? "warning" : "success" });
            }}
          />
        </Overlay>
      )}
      {existingRepoPrompt !== null && (
        <ConfirmDialog
          ariaLabel="Repository already exists"
          message={<p>{existingRepoPrompt} is already a repository. Open it?</p>}
          confirmLabel="Open"
          onConfirm={() => {
            void openRepoTab(existingRepoPrompt);
            setExistingRepoPrompt(null);
          }}
          onCancel={() => setExistingRepoPrompt(null)}
        />
      )}
      <ToastRegion toasts={appToasts.toasts} onDismiss={appToasts.dismiss} onPauseChange={appToasts.setPaused} />
      {openRepos.openRepos.length === 0 ? (
        <RepoPicker
          client={client}
          onOpenRepo={openRepoTab}
          onOpenWorkspace={(workspace) => void openRepos.openWorkspace(workspace)}
          workspaces={workspaces.workspaces}
          aliases={openRepos.aliases}
          workspacesLoading={workspaces.loading}
          workspacesError={workspaces.error}
          onDismissWorkspacesError={workspaces.dismissError}
          onCreateWorkspace={workspaces.createWorkspace}
          onEditWorkspace={workspaces.editWorkspace}
          onDeleteWorkspace={workspaces.deleteWorkspace}
          onCloneRepository={appCommands?.onCloneRepository}
          onNewRepository={appCommands?.onNewRepository}
        />
      ) : (
        // PERF-001: only the active tab's workspace is mounted — an inactive one renders nothing
        // rather than staying mounted `display: none`. `RepoTabs` above still renders every open
        // tab (so switching away and back is possible); this just avoids paying N tabs' worth of
        // steady-state DOM/IPC cost while only one is ever visible. Switching back remounts a
        // fresh `RepoWorkspace`, which re-fetches its own status/graph on mount — already the
        // existing behavior for a newly-opened repo, so no new loading-state code is needed.
        openRepos.openRepos
          .filter((repo) => repo.path === openRepos.activePath)
          .map((repo) => (
            <RepoWorkspace
              key={repo.path}
              repoPath={repo.path}
              active
              client={client}
              onOpenRepoTab={openRepoTab}
              onBusyChange={onBusyChange}
              openRepos={openRepos.openRepos}
              onSwitchRepoTab={openRepos.switchTo}
              onCloseRepoTab={openRepos.closeRepo}
              getCommitDraft={getCommitDraft}
              onCommitDraftChange={onCommitDraftChange}
              appCommands={appCommands}
              pushToast={pushAppToast}
              onStatusChange={tabChangeMarkers.onLiveStatus}
            />
          ))
      )}
    </main>
  );
}
