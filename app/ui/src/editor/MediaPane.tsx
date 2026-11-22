// The media pane (T4): the recording (with its picture when it has one), a waveform that follows the playhead, an
// overview of the whole file with the speakers' colour band (T3) and how far it has been checked, and the transport.
import { useEffect, useRef, useState } from 'react';
import { Button } from 'react-aria-components';
import type { EditorInfo, Turn } from '../api';
import { hms } from '../format';

export interface PlayerState {
  time: number;
  playing: boolean;
  rate: number;
  boost: number;
}

const WINDOW_SECONDS = 16;

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || '#888';
}

function useCanvas(draw: (ctx: CanvasRenderingContext2D, w: number, h: number) => void, deps: unknown[]) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth;
    const h = c.clientHeight;
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
    }
    const ctx = c.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    draw(ctx, w, h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return ref;
}

export function MediaPane(props: {
  info: EditorInfo;
  peaks: number[] | null;
  turns: Turn[];
  speakerNumber: (id: string) => number;
  checkedUntil: number | null;
  player: PlayerState;
  mediaRef: React.RefObject<HTMLVideoElement | null>;
  onSeek: (t: number) => void;
  onTogglePlay: () => void;
  onSkip: (seconds: number) => void;
  onRate: (r: number) => void;
  onBoost: (b: number) => void;
  keyLabel: (action: string) => string;
}) {
  const { info, peaks, player } = props;
  const [playError, setPlayError] = useState(false);
  const pps = info.peaks_per_second;
  const duration = Math.max(info.duration, 0.1);

  const wave = useCanvas(
    (ctx, w, h) => {
      const start = player.time - WINDOW_SECONDS / 2;
      const x = (t: number) => ((t - start) / WINDOW_SECONDS) * w;
      // Speaker band along the bottom.
      for (const t of props.turns) {
        if (t.end < start || t.start > start + WINDOW_SECONDS) continue;
        ctx.fillStyle = cssVar(`--s${((props.speakerNumber(t.speaker) - 1) % 8) + 1}`);
        ctx.fillRect(x(t.start), h - 6, Math.max(1, x(t.end) - x(t.start)), 6);
      }
      if (props.checkedUntil) {
        ctx.fillStyle = cssVar('--checked');
        ctx.fillRect(0, 0, Math.max(0, x(props.checkedUntil)), h - 8);
      }
      if (peaks) {
        ctx.fillStyle = cssVar('--wave');
        const mid = (h - 8) / 2;
        for (let px = 0; px < w; px++) {
          const t0 = start + (px / w) * WINDOW_SECONDS;
          const t1 = start + ((px + 1) / w) * WINDOW_SECONDS;
          const i0 = Math.max(0, Math.floor(t0 * pps));
          const i1 = Math.min(peaks.length / 2, Math.ceil(t1 * pps));
          let lo = 0;
          let hi = 0;
          for (let i = i0; i < i1; i++) {
            lo = Math.min(lo, peaks[2 * i]);
            hi = Math.max(hi, peaks[2 * i + 1]);
          }
          if (i1 > i0) ctx.fillRect(px, mid - (hi / 127) * mid, 1, Math.max(1, ((hi - lo) / 127) * mid));
        }
      }
      ctx.fillStyle = cssVar('--accent');
      ctx.fillRect(w / 2 - 1, 0, 2, h);
    },
    [peaks, player.time, props.turns, props.checkedUntil],
  );

  const overview = useCanvas(
    (ctx, w, h) => {
      for (const t of props.turns) {
        ctx.fillStyle = cssVar(`--s${((props.speakerNumber(t.speaker) - 1) % 8) + 1}`);
        ctx.fillRect((t.start / duration) * w, 0, Math.max(1, ((t.end - t.start) / duration) * w), h - 4);
      }
      if (props.checkedUntil) {
        ctx.fillStyle = cssVar('--accent');
        ctx.fillRect(0, h - 3, (props.checkedUntil / duration) * w, 3);
      }
      ctx.strokeStyle = cssVar('--text');
      ctx.lineWidth = 1;
      const vx = ((player.time - WINDOW_SECONDS / 2) / duration) * w;
      ctx.strokeRect(vx, 0.5, Math.max(2, (WINDOW_SECONDS / duration) * w), h - 1);
    },
    [props.turns, player.time, props.checkedUntil, duration],
  );

  const seekFromCanvas = (e: React.MouseEvent<HTMLCanvasElement>, whole: boolean) => {
    const r = e.currentTarget.getBoundingClientRect();
    const f = (e.clientX - r.left) / r.width;
    props.onSeek(whole ? f * duration : player.time - WINDOW_SECONDS / 2 + f * WINDOW_SECONDS);
  };

  return (
    <aside className="media-pane" aria-label="Recording">
      <video
        ref={props.mediaRef}
        src={info.url}
        crossOrigin="anonymous"
        preload="auto"
        className={info.has_video ? 'video' : 'audio-only'}
        aria-label={info.has_video ? 'Video of the recording' : undefined}
        onError={() => setPlayError(true)}
      />
      {info.missing ? (
        <p className="warning" role="alert">
          The recording is not where it was imported from (<span className="path">{info.missing}</span>), so it cannot be played. Put it back there and open
          the transcript again. The words can still be edited and exported.
        </p>
      ) : (
        playError && (
          <p className="warning" role="alert">
            This recording could not be played. The words can still be edited and exported.
          </p>
        )
      )}
      {info.playback_copy && <p className="hint">Playing a sound-only copy: this file type cannot be played directly.</p>}
      <canvas ref={wave} className="wave" onClick={(e) => seekFromCanvas(e, false)} aria-hidden="true" />
      <canvas ref={overview} className="overview" onClick={(e) => seekFromCanvas(e, true)} aria-hidden="true" />
      <div className="transport">
        <span className="clock" aria-live="off">
          {hms(player.time)} / {hms(duration)}
        </span>
        <div className="row">
          <Button onPress={() => props.onSkip(-2)} aria-label={`Back 2 seconds (${props.keyLabel('back')})`}>
            ⏪
          </Button>
          <Button className="primary" onPress={props.onTogglePlay} aria-label={`${player.playing ? 'Pause' : 'Play'} (${props.keyLabel('play_pause')})`}>
            {player.playing ? 'Pause' : 'Play'}
          </Button>
          <Button onPress={() => props.onSkip(2)} aria-label={`Forward 2 seconds (${props.keyLabel('forward')})`}>
            ⏩
          </Button>
        </div>
        <div className="row">
          <Button onPress={() => props.onRate(player.rate - 0.1)} isDisabled={player.rate <= 0.5} aria-label={`Slower (${props.keyLabel('slower')})`}>
            −
          </Button>
          <span className="rate" role="status" aria-label={`Speed ${player.rate.toFixed(1)} times`}>
            Speed {player.rate.toFixed(1)}×
          </span>
          <Button onPress={() => props.onRate(player.rate + 0.1)} isDisabled={player.rate >= 2.0} aria-label={`Faster (${props.keyLabel('faster')})`}>
            +
          </Button>
        </div>
        <label className="boost">
          Volume boost
          <select value={player.boost} onChange={(e) => props.onBoost(Number(e.target.value))}>
            <option value={1}>Off</option>
            <option value={1.5}>1.5×</option>
            <option value={2}>2×</option>
            <option value={3}>3×</option>
            <option value={4}>4×</option>
          </select>
        </label>
      </div>
    </aside>
  );
}
