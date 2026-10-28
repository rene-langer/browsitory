import { useCallback, useEffect, useState } from "react";
import type { DiffSource, FileSide, FileSides, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { formatBytes, imageDataUrl } from "../lib/imageTypes";
import styles from "./ImageDiff.module.css";
import { InlineError } from "./primitives/InlineError";

/**
 * Side-by-side "Before" / "After" preview of an image file's diff. The bytes come from
 * `getFileSides` as base64 and are shown through `data:` URLs in `<img>` only, with the image type
 * the backend sniffed from the bytes, so an SVG is never a live document that can run script.
 */
export function ImageDiff({
  client,
  repoPath,
  path,
  source,
  refreshKey = 0,
}: {
  client: RepoClient;
  repoPath: string;
  path: string;
  source: DiffSource;
  /** A change refetches (the working tree or index may have changed). */
  refreshKey?: number | string;
}) {
  const [sides, setSides] = useState<FileSides | null>(null);
  const [error, setError] = useState<DescribedError | null>(null);
  const [attempt, setAttempt] = useState(0);
  const sourceKey = JSON.stringify(source);

  useEffect(() => {
    let ignore = false;
    Promise.resolve()
      .then(() => client.getFileSides(repoPath, path, JSON.parse(sourceKey) as DiffSource))
      .then(
        (next) => {
          if (ignore) return;
          setSides(next);
          setError(null);
        },
        (err: unknown) => {
          if (!ignore) setError(describeError(err));
        },
      );
    return () => {
      ignore = true;
    };
  }, [client, repoPath, path, sourceKey, refreshKey, attempt]);

  const retry = useCallback(() => {
    setError(null);
    setAttempt((n) => n + 1);
  }, []);

  if (error !== null) {
    return <InlineError message={error.message} hint={error.hint} onDismiss={() => setError(null)} onRetry={retry} />;
  }
  if (sides === null) {
    return (
      <p role="status" className={styles.loading}>
        Loading preview…
      </p>
    );
  }
  const bothMissing = sides.old.kind === "Missing" && sides.new.kind === "Missing";
  return (
    <div className={styles.columns}>
      <Side label="Before" path={path} side={sides.old} missingText={bothMissing ? "Not available" : "Added"} />
      <Side label="After" path={path} side={sides.new} missingText={bothMissing ? "Not available" : "Deleted"} />
    </div>
  );
}

function Side({
  label,
  path,
  side,
  missingText,
}: {
  label: "Before" | "After";
  path: string;
  side: FileSide;
  missingText: string;
}) {
  // Keyed by the image data so a refetch with new bytes starts fresh.
  const key = side.kind === "Loaded" ? `${side.mime}:${side.size}:${side.base64.length}:${side.base64.slice(0, 64)}` : side.kind;
  return (
    <figure className={styles.side}>
      <figcaption className={styles.heading}>{label}</figcaption>
      <SideBody key={key} label={label} path={path} side={side} missingText={missingText} />
    </figure>
  );
}

function SideBody({
  label,
  path,
  side,
  missingText,
}: {
  label: string;
  path: string;
  side: FileSide;
  missingText: string;
}) {
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);
  const [failed, setFailed] = useState(false);
  if (side.kind === "Missing") return <p className={styles.placeholder}>{missingText}</p>;
  if (side.kind === "TooLarge") {
    return <p className={styles.placeholder}>Too large to preview ({formatBytes(side.size)})</p>;
  }
  const url = imageDataUrl(side.mime, side.base64);
  if (url === null || failed) return <p className={styles.placeholder}>Can't display this image</p>;
  return (
    <>
      <div className={styles.frame}>
        <img
          className={styles.image}
          alt={`${label}: ${path}`}
          src={url}
          onLoad={(event) =>
            setSize({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight })
          }
          onError={() => setFailed(true)}
        />
      </div>
      <p className={styles.meta}>
        {size === null ? formatBytes(side.size) : `${size.width} × ${size.height} px · ${formatBytes(side.size)}`}
      </p>
    </>
  );
}

/** An SVG's "Show text diff" / "Show image" switch, above its preview or hunks. */
export function ImageTextToggle({ asText, onToggle }: { asText: boolean; onToggle: () => void }) {
  return (
    <div className={styles.toggle}>
      <button type="button" onClick={onToggle}>
        {asText ? "Show image" : "Show text diff"}
      </button>
    </div>
  );
}
