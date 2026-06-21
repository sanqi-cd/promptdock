export interface Prompt {
  id: string;
  title: string;
  content: string;
  optimized_content: string | null;
  category: string | null;
  summary: string | null;
  is_favorite: boolean;
  created_at: string;
  updated_at: string;
  last_used_at: string | null;
}

export interface Tag {
  id: string;
  name: string;
}

export interface SearchResult {
  id: string;
  title: string;
  summary: string | null;
  category: string | null;
  tags: string[];
  updated_at: string;
}

export interface ClassifyResult {
  category: string;
  tags: string[];
  summary: string;
  recommended_tool: string;
}

export interface PolishProcessStep {
  title: string;
  desc: string;
}

export interface ApiConfig {
  id: string;
  name: string;
  provider: string;
  base_url: string;
  model: string;
  is_active: boolean;
  created_at: string;
}

export type PolishMode = "general" | "structured" | "professional" | "concise";
