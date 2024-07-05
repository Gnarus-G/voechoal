export type AudioItem = {
  id: string;
  excerpt: string;
  filepath: string;
  is_playing: boolean;
};

export type PollingState = {
  is_transcribing: boolean;
  audio_items: AudioItem[];
};
