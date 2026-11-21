export type EditTool = 'trim' | 'ripple' | 'roll' | 'slip' | 'slide';

export interface AssetInfo {
  id: string;
  name: string;
  kind: string;
  media?: { has_audio?: boolean; duration_ms?: number } | null;
}

export interface Transform {
  fit?: string;
  motion?: string;
  fade_in_ms?: number;
  fade_out_ms?: number;
  audio_fade_in_ms?: number;
  audio_fade_out_ms?: number;
  [key: string]: unknown;
}

export interface Clip {
  id: string;
  asset_id: string;
  track_id: string;
  start_ms: number;
  trim_in_ms: number;
  duration_ms: number;
  label?: string | null;
  volume?: number;
  opacity?: number;
  transform?: Transform;
}

export interface Track {
  id: string;
  name: string;
  kind: string;
  visible: boolean;
  locked: boolean;
}

export interface ProjectFull {
  id: string;
  title: string;
  updated_at: string;
  timeline: { tracks: Track[]; clips: Clip[] };
  assets: AssetInfo[];
}
