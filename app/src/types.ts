export interface ProjectSnapshot {
  id: string;
  name: string;
  path: string;
  aspectRatio: "9:16" | "16:9";
  platformTarget: string;
  script: string;
  blocks: ScriptBlock[];
  assets: MediaAsset[];
  settings: ProjectSettings;
}
export interface ProjectSettings {
  backgroundMusicAssetId: string | null;
  musicVolume: number;
  musicDucking: boolean;
  openingCard: boolean;
  openingTitle: string;
  captionStyle: "clean" | "bold" | "minimal";
  transitionStyle: "cut" | "dissolve";
}
export interface ScriptBlock {
  id: string;
  position: number;
  text: string;
  status: "prepared" | "recorded" | "reviewed";
  alignmentStale: boolean;
  tray: TrayItem[];
  takes: Take[];
}
export interface Take {
  id: string;
  relativePath: string;
  processedRelativePath: string | null;
  durationUs: number;
  selected: boolean;
  createdAt: string;
  alignmentTotal: number;
  alignmentMatched: number;
  transcript: string | null;
}
export interface MediaAsset {
  id: string;
  name: string;
  relativePath: string;
  mediaType: "image" | "video" | "audio";
  contentHash: string;
  durationUs: number | null;
  width: number | null;
  height: number | null;
  proxyRelativePath: string | null;
  thumbnailRelativePath: string | null;
}
export interface TrayItem {
  id: string;
  assetId: string;
  position: number;
  playbackMode: "narrate_over" | "play_solo";
  inPointUs: number;
  outPointUs: number | null;
  loopMode: "freeze" | "repeat";
}
