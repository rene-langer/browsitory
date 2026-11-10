import { USER_GUIDE_URL } from "./lib/links";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Moon, Settings, Sparkles, Sun } from "lucide-react";
import { ShortcutHint } from "./components/ShortcutHint";
import { RepoPicker } from "./components/RepoPicker";
import { RepoTabs } from "./components/RepoTabs";
import { ReleaseNotesModal } from "./components/ReleaseNotesModal";
import { ThirdPartyLicencesDialog } from "./components/ThirdPartyLicencesDialog";
import { ToastRegion } from "./components/primitives/ToastRegion";
import { useToasts } from "./state/useToasts";
import { InlineError } from "./components/primitives/InlineError";
import { ConfirmDialog } from "./components/primitives/ConfirmDialog";
import { CloneDialog } from "./components/CloneDialog";
import { Overlay } from "./components/primitives/Overlay";
import type { RepoClient } from "./ipc/RepoClient";
import { subscribeTransportStatus } from "./ipc/transportStatus";
import type { AppCommands } from "./lib/commands";
import { CommitMessageSection, DiffSection, ExternalToolsSection, FetchSection, PreferencesDialog } from "./components/PreferencesDialog";
import { cloneToastMessage, createNewRepository } from "./lib/newRepository";
import { repoNameOf } from "./lib/repoName";
import { persistTheme } from "./lib/theme";
import { CommitMessageSettingsProvider } from "./state/CommitMessageSettingsProvider";
import { FetchSettingsProvider } from "./state/FetchSettingsProvider";
import { DiffSettingsProvider } from "./state/DiffSettingsProvider";
import { useOpenRepos } from "./state/useOpenRepos";
import { useTabChangeMarkers } from "./state/useTabChangeMarkers";
import { useWorkspaces } from "./state/useWorkspaces";
import styles from "./App.module.css";
import { RepoWorkspace } from "./RepoWorkspace";
import { useBusyPaths } from "./state/useBusyPaths";
import { useCommitDrafts } from "./state/useCommitDrafts";
import { useE2eAutoOpen } from "./state/useE2eAutoOpen";
import { allReleaseNotes, useReleaseNotes } from "./state/useReleaseNotes";
import { useTheme } from "./state/useTheme";

export default function App(props: {
  client: RepoClient;
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
          <AppContent {...props} />
        </FetchSettingsProvider>
      </CommitMessageSettingsProvider>
    </DiffSettingsProvider>
  );
}

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
  const [theme, setTheme] = useTheme();
  const { onBusyChange, busyPaths } = useBusyPaths();
  const { getCommitDraft, onCommitDraftChange } = useCommitDrafts();

  const [pickingRepo, setPickingRepo] = useState(false);

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
    }),
    [client.supportsCloneAndInit, startClone, startNewRepository, openPreferences, openLicences, openUserGuide, canReopenClosedRepo, reopenClosedTab],
  );
  const tabMenuItems = useCallback(
    (path: string) => {
      const run = (action: () => Promise<void>) => {
        setTabToolError(null);
        action().catch((error: unknown) => setTabToolError(error instanceof Error ? error.message : String(error)));
      };
      return [
        { label: "Open terminal here", onSelect: () => run(() => client.openTerminal(path)) },
        { label: "Reveal repository", onSelect: () => run(() => client.revealInFileManager(path, null)) },
        {
          label: "Reopen closed tab",
          onSelect: reopenClosedTab,
          disabled: !canReopenClosedRepo,
          title: canReopenClosedRepo ? undefined : "No closed tab to reopen",
        },
      ];
    },
    [client, reopenClosedTab, canReopenClosedRepo],
  );

  const { releaseNotesView, setReleaseNotesView, closeReleaseNotes } = useReleaseNotes(client);

  const themeToggle = (
    <button
      type="button"
      className={styles.iconButton}
      aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
      onClick={() => {
        const next = theme === "dark" ? "light" : "dark";
        setTheme(next);
        persistTheme(next);
      }}
    >
      {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
    </button>
  );

  useE2eAutoOpen(openRepos, openRepoTab);

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
      {updateBanner}
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
          changedPaths={tabChangeMarkers.changedPaths}
        />
        {openRepos.openRepos.length === 0 && <div className={styles.headerSpacer} />}
        {openRepos.openRepos.length > 0 && <ShortcutHint />}
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
      </header>
      {openRepos.restoreError !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={openRepos.restoreError} onDismiss={openRepos.dismissRestoreError} />
        </div>
      )}
      {tabToolError !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={tabToolError} onDismiss={() => setTabToolError(null)} />
        </div>
      )}
      {preferencesOpen && (
        <PreferencesDialog onClose={() => setPreferencesOpen(false)}>
          <ExternalToolsSection client={client} repoPath={openRepos.activePath} toolHost={toolHost} />
          <CommitMessageSection />
          <FetchSection repoPath={openRepos.activePath} />
          <DiffSection />
        </PreferencesDialog>
      )}
      {(transportError !== null || openError !== null) && (
        <div className={styles.errorLayer}>
          {transportError !== null && (
            <InlineError
              message={transportError}
              hint="Browsitory lost its connection to the backend. Restart the app if this keeps happening."
              onDismiss={() => setTransportError(null)}
            />
          )}
          {openError !== null && (
            <InlineError
              message={openError.message}
              hint={openError.path !== "" ? "Check that the folder exists and is a Git repository." : undefined}
              onRetry={openError.path !== "" ? () => void openRepoTab(openError.path) : undefined}
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
      {pickingRepo && (
        <Overlay onClose={() => setPickingRepo(false)}>
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
              pushAppToast(cloneToastMessage(outcome, name));
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
      <ToastRegion toasts={appToasts.toasts} onDismiss={appToasts.dismiss} />
      {openRepos.openRepos.length === 0 ? (
        <RepoPicker
          client={client}
          onOpenRepo={openRepoTab}
          onOpenWorkspace={(workspace) => void openRepos.openWorkspace(workspace)}
          workspaces={workspaces.workspaces}
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
