// The batch queue (T8): files waiting to be transcribed, one at a time, in an order the user sets by dragging (or
// with the move buttons, by keyboard). Run, pause, resume and cancel for the whole queue; hold or remove one file.
// The queue lives in the project file, so it survives a restart; each file carries on from its last finished chunk.
import { Button, GridList, GridListItem, useDragAndDrop } from 'react-aria-components';
import { api, type MediaFile, type Queue, type QueueItem, type QueueSummary } from './api';

const STATE_WORDS: Record<QueueItem['state'], string> = {
  waiting: 'Waiting',
  running: 'Transcribing',
  held: 'Held',
  done: 'Done',
  failed: 'Failed',
  cancelled: 'Cancelled',
};

function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
}

export function QueuePanel(props: {
  queue: Queue;
  files: MediaFile[];
  summary: QueueSummary | null;
  onQueue: (q: Queue) => void;
  onNotice: (s: string) => void;
  onDismissSummary: () => void;
}) {
  const q = props.queue;
  const label = (id: string) => props.files.find((f) => f.id === id)?.label ?? id;
  const act = (p: Promise<Queue>) => p.then(props.onQueue).catch((e) => props.onNotice(message(e)));
  const waiting = q.items.filter((i) => i.state === 'waiting').length;
  const running = q.items.find((i) => i.state === 'running');
  const finished = q.items.some((i) => i.state === 'done' || i.state === 'failed' || i.state === 'cancelled');
  const { dragAndDropHooks } = useDragAndDrop({
    getItems: (keys) => [...keys].map((k) => ({ 'text/plain': label(String(k)) })),
    onReorder(e) {
      const id = String([...e.keys][0]);
      const others = q.items.filter((i) => i.id !== id).map((i) => i.id);
      const at = others.indexOf(String(e.target.key));
      act(api.queueMove(id, e.target.dropPosition === 'after' ? at + 1 : at));
    },
  });
  if (q.items.length === 0 && !props.summary) return null;
  return (
    <section className="queue" aria-labelledby="queue-heading">
      <div className="section-head">
        <h3 id="queue-heading">
          Queue{' '}
          <span className="hint">
            {q.state === 'running' ? (running ? `transcribing ${label(running.id)}` : 'running') : q.state === 'paused' ? 'paused' : 'not running'}
            {waiting ? ` · ${waiting} waiting` : ''}
          </span>
        </h3>
        <div className="row">
          {q.state !== 'running' && (waiting > 0 || running) && (
            <Button className="primary" onPress={() => act(api.queueStart())}>
              {q.state === 'paused' ? 'Resume' : 'Run'}
            </Button>
          )}
          {q.state === 'running' && <Button onPress={() => act(api.queuePause())}>Pause</Button>}
          {(waiting > 0 || running) && <Button onPress={() => act(api.queueCancel())}>Cancel all</Button>}
          {finished && (
            <Button className="quiet" onPress={() => act(api.queueClear())}>
              Clear finished
            </Button>
          )}
        </div>
      </div>
      {q.interrupted && (
        <p className="warning" role="status">
          The queue stopped when Prism Transcribe closed. Resume carries on from where each file stopped.
        </p>
      )}
      {props.summary && (
        <div className="queue-summary" role="status">
          <p>
            The queue has finished: {props.summary.done.length} transcribed
            {props.summary.failed.length ? `, ${props.summary.failed.length} failed` : ''}, in {Math.max(1, Math.round(props.summary.seconds / 60))} min.
          </p>
          {props.summary.failed.length > 0 && (
            <ul>
              {props.summary.failed.map(([l, why]) => (
                <li key={l}>
                  {l}: {why}
                </li>
              ))}
            </ul>
          )}
          <Button className="quiet" onPress={props.onDismissSummary}>
            Dismiss
          </Button>
        </div>
      )}
      {q.items.length > 0 && (
        <GridList aria-label="Files in the queue, in the order they run" items={q.items} dragAndDropHooks={dragAndDropHooks} className="queue-list">
          {(item) => {
            const i = q.items.indexOf(item);
            const open = item.state === 'waiting' || item.state === 'held';
            return (
              <GridListItem id={item.id} textValue={`${label(item.id)}, ${STATE_WORDS[item.state]}`} className={`queue-item ${item.state}`}>
                <Button slot="drag" className="quiet drag" aria-label={`Drag ${label(item.id)}`}>
                  ⠿
                </Button>
                <span className="queue-label">{label(item.id)}</span>
                <span className="queue-state">
                  {STATE_WORDS[item.state]}
                  {item.error ? `: ${item.error}` : ''}
                </span>
                <span className="row">
                  {open && i > 0 && (
                    <Button className="quiet" aria-label={`Move ${label(item.id)} up`} onPress={() => act(api.queueMove(item.id, i - 1))}>
                      ↑
                    </Button>
                  )}
                  {open && i < q.items.length - 1 && (
                    <Button className="quiet" aria-label={`Move ${label(item.id)} down`} onPress={() => act(api.queueMove(item.id, i + 1))}>
                      ↓
                    </Button>
                  )}
                  {(item.state === 'waiting' || item.state === 'running') && (
                    <Button className="quiet" onPress={() => act(api.queueHold(item.id, true))}>
                      Hold
                    </Button>
                  )}
                  {item.state === 'held' && (
                    <Button className="quiet" onPress={() => act(api.queueHold(item.id, false))}>
                      Release
                    </Button>
                  )}
                  <Button className="quiet" aria-label={`Remove ${label(item.id)} from the queue`} onPress={() => act(api.queueRemove(item.id))}>
                    Remove
                  </Button>
                </span>
              </GridListItem>
            );
          }}
        </GridList>
      )}
    </section>
  );
}
