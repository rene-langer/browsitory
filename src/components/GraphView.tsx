import { useMemo } from 'react'
import dagre from 'dagre'
import clsx from 'clsx'
import type { GraphCommit } from '@services/git'

interface GraphViewProps {
  commits: GraphCommit[]
  selectedOid?: string | null
  onSelect: (oid: string) => void
}

// Dagre computes DAG node positions/edge routing only — rendering is a
// hand-written SVG renderer below, deliberately avoiding a full
// interactive-canvas library like React Flow to keep the dependency
// footprint small (see docs/LICENSE_COMPLIANCE.md).
const NODE_WIDTH = 260
const NODE_HEIGHT = 44
const NODE_RADIUS = 6

interface PositionedNode {
  commit: GraphCommit
  x: number
  y: number
}

interface PositionedEdge {
  key: string
  points: { x: number; y: number }[]
}

function truncate(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text
}

export default function GraphView({ commits, selectedOid, onSelect }: GraphViewProps) {
  const layout = useMemo(() => {
    const g = new dagre.graphlib.Graph()
    g.setGraph({ rankdir: 'TB', nodesep: 20, ranksep: 44, marginx: 24, marginy: 24 })
    g.setDefaultEdgeLabel(() => ({}))

    const byOid = new Map(commits.map((c) => [c.oid, c]))
    for (const commit of commits) {
      g.setNode(commit.oid, { width: NODE_WIDTH, height: NODE_HEIGHT })
    }
    for (const commit of commits) {
      for (const parentOid of commit.parents) {
        if (byOid.has(parentOid)) g.setEdge(commit.oid, parentOid)
      }
    }

    dagre.layout(g)

    const nodes: PositionedNode[] = commits.map((commit) => {
      const pos = g.node(commit.oid)
      return { commit, x: pos.x - NODE_WIDTH / 2, y: pos.y }
    })
    const edges: PositionedEdge[] = g.edges().map((e) => ({
      key: `${e.v}->${e.w}`,
      points: g.edge(e).points.map((p) => ({ x: p.x - NODE_WIDTH / 2, y: p.y })),
    }))

    const graphLabel = g.graph()
    return {
      nodes,
      edges,
      width: (graphLabel.width ?? 0) + 24,
      height: (graphLabel.height ?? 0) + 24,
    }
  }, [commits])

  if (commits.length === 0) {
    return <p className="p-4 text-sm text-muted-foreground">No commits yet.</p>
  }

  return (
    <div className="overflow-auto p-4">
      <svg width={layout.width} height={layout.height} role="img" aria-label="Commit graph">
        {layout.edges.map((edge) => (
          <polyline
            key={edge.key}
            points={edge.points.map((p) => `${p.x},${p.y}`).join(' ')}
            fill="none"
            className="stroke-border"
            strokeWidth={2}
          />
        ))}
        {layout.nodes.map(({ commit, x, y }) => {
          const selected = commit.oid === selectedOid
          const date = new Date(commit.author.timestamp).toLocaleString()
          const firstLine = commit.message.split('\n')[0]
          return (
            <g
              key={commit.oid}
              transform={`translate(${x}, ${y})`}
              className="cursor-pointer"
              onClick={() => onSelect(commit.oid)}
            >
              <title>{`${commit.oid}\n${commit.author.name} <${commit.author.email}> · ${date}`}</title>
              {commit.refs.length > 0 && (
                <text
                  x={NODE_RADIUS * 2 + 8}
                  y={-8}
                  className="fill-git-green text-[10px] font-semibold"
                >
                  {commit.refs.join(', ')}
                </text>
              )}
              <circle
                cx={NODE_RADIUS}
                cy={0}
                r={NODE_RADIUS}
                className={clsx(
                  'stroke-2',
                  selected ? 'fill-primary stroke-primary' : 'fill-git-blue stroke-git-blue'
                )}
              />
              <text
                x={NODE_RADIUS * 2 + 8}
                y={4}
                className={clsx(
                  'font-mono text-xs',
                  selected ? 'fill-primary font-semibold' : 'fill-foreground'
                )}
              >
                {commit.oid.slice(0, 7)}
              </text>
              <text x={NODE_RADIUS * 2 + 8} y={18} className="fill-muted-foreground text-xs">
                {truncate(firstLine, 36)}
              </text>
            </g>
          )
        })}
      </svg>
    </div>
  )
}
