export interface ProjectSnapshot { id: string; name: string; path: string; aspectRatio: '9:16' | '16:9'; platformTarget: string; script: string; blocks: ScriptBlock[]; assets: MediaAsset[] }
export interface ScriptBlock { id: string; position: number; text: string; status: 'prepared' | 'recorded' | 'reviewed'; alignmentStale: boolean; tray: TrayItem[] }
export interface MediaAsset { id: string; name: string; relativePath: string; mediaType: 'image' | 'video' | 'audio'; contentHash: string }
export interface TrayItem { id: string; assetId: string; position: number; playbackMode: 'narrate_over' | 'play_solo'; inPointUs: number; outPointUs: number | null }
