export type AudioItem = {
  id: string;
  excerpt: string;
  filepath: string;
};

export type PollingState = {
  is_transcribing: boolean;
  audio_items: AudioItem[];
};
