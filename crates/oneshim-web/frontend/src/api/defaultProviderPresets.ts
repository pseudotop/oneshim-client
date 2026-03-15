import type { ProviderPresetCatalog } from './client'

export const DEFAULT_PROVIDER_PRESET_CATALOG: ProviderPresetCatalog = {
  version: 2,
  updated_at: '2026-03-15T18:30:00Z',
  providers: [
    {
      provider_type: 'Anthropic',
      aliases: ['anthropic'],
      display_name: 'Anthropic',
      llm_endpoint: 'https://api.anthropic.com/v1/messages',
      ocr_endpoint: 'https://api.anthropic.com/v1/messages',
      model_catalog_endpoint: 'https://api.anthropic.com/v1/models',
      ocr_model_catalog_supported: true,
      llm_models: ['claude-sonnet-4-20250514', 'claude-opus-4-1-20250805'],
      ocr_models: ['claude-sonnet-4-20250514', 'claude-opus-4-1-20250805'],
    },
    {
      provider_type: 'OpenAi',
      aliases: ['openai', 'open_ai', 'open-ai', 'openai-compatible'],
      display_name: 'OpenAI',
      llm_endpoint: 'https://api.openai.com/v1/responses',
      ocr_endpoint: 'https://api.openai.com/v1/chat/completions',
      model_catalog_endpoint: 'https://api.openai.com/v1/models',
      ocr_model_catalog_supported: true,
      llm_models: ['gpt-5-mini', 'gpt-5.2', 'gpt-5-nano'],
      ocr_models: ['gpt-5-mini', 'gpt-5.2'],
    },
    {
      provider_type: 'Google',
      aliases: ['google', 'gemini'],
      display_name: 'Google',
      llm_endpoint:
        'https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent',
      ocr_endpoint: 'https://vision.googleapis.com/v1/images:annotate',
      model_catalog_endpoint: 'https://generativelanguage.googleapis.com/v1beta/models',
      ocr_model_catalog_supported: false,
      ocr_model_catalog_notice: 'Google Vision OCR endpoint does not expose a selectable model catalog.',
      llm_models: ['gemini-2.5-flash', 'gemini-2.5-flash-lite', 'gemini-2.5-pro'],
      ocr_models: [],
    },
    {
      provider_type: 'Generic',
      aliases: ['generic'],
      display_name: 'Generic',
      llm_endpoint: 'https://api.openai.com/v1/chat/completions',
      ocr_endpoint: 'https://api.openai.com/v1/chat/completions',
      model_catalog_endpoint: 'https://api.openai.com/v1/models',
      ocr_model_catalog_supported: true,
      llm_models: ['gpt-5-mini', 'gpt-5-nano'],
      ocr_models: ['gpt-5-mini'],
    },
  ],
}

export const DEFAULT_PROVIDER_PRESETS = DEFAULT_PROVIDER_PRESET_CATALOG.providers
