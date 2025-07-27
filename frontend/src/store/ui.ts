import { create } from 'zustand'

interface UIState {
  sidebarOpen: boolean
  setSidebarOpen: (open: boolean) => void
  currentPage: string
  setCurrentPage: (page: string) => void
  loading: boolean
  setLoading: (loading: boolean) => void
}

export const useUI = create<UIState>((set) => ({
  sidebarOpen: true,
  setSidebarOpen: (open) => set({ sidebarOpen: open }),
  currentPage: 'dashboard',
  setCurrentPage: (page) => set({ currentPage: page }),
  loading: false,
  setLoading: (loading) => set({ loading }),
}))