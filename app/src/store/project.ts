import { create } from 'zustand'
import type { ProjectSnapshot } from '../types'
function remember(project: ProjectSnapshot) {
  const recent = JSON.parse(localStorage.getItem('cheeza.recent') ?? '[]') as Array<{ name: string; path: string; aspectRatio: string }>;
  localStorage.setItem('cheeza.recent', JSON.stringify([{ name: project.name, path: project.path, aspectRatio: project.aspectRatio }, ...recent.filter((item) => item.path !== project.path)].slice(0, 6)));
}
interface ProjectState { project: ProjectSnapshot | null; activeBlockId: string | null; busy: boolean; error: string | null; setProject: (project: ProjectSnapshot) => void; setActiveBlock: (id: string) => void; setBusy: (busy: boolean) => void; setError: (error: string | null) => void; reset: () => void }
export const useProjectStore = create<ProjectState>((set) => ({ project: null, activeBlockId: null, busy: false, error: null, setProject: (project) => { remember(project); set((state) => ({ project, activeBlockId: project.blocks.some((block) => block.id === state.activeBlockId) ? state.activeBlockId : project.blocks[0]?.id ?? null })); }, setActiveBlock: (activeBlockId) => set({ activeBlockId }), setBusy: (busy) => set({ busy }), setError: (error) => set({ error }), reset: () => set({ project: null, activeBlockId: null, error: null }) }))
