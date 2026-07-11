# Design — Tela de configuração, aparência customizável e animações

**Data:** 2026-07-06
**Branch:** `feat/settings-window`
**Status:** aprovado para escrever plano de implementação

## 1. Objetivo

Adicionar ao Claude Code Usage Monitor uma **janela de configuração** (bespoke, desenhada
em GDI, no estilo do widget) que permite customizar a aparência do widget da taskbar —
cores (RGBA), fonte, tamanho/geometria — e um conjunto de **animações fluidas selecionáveis**,
além da frequência de atualização. Uso pessoal agora; publicação futura para a comunidade
creditando o projeto original (CodeZeno).

## 2. Restrições de arquitetura (estado atual)

- O widget é uma **janela *layered* embutida na taskbar** (~276×58 px lógicos), desenhada
  com **GDI** dentro de um DIB 32bpp top-down e composta via `UpdateLayeredWindow`
  (`render_layered` → `paint_content` em `window.rs`). Per-pixel alpha disponível.
- **Sem GPU** no caminho de render. Animações são desenhadas na CPU via timer. "Sofisticado"
  aqui = refinado e leve (não efeitos de jogo/blur pesado/partículas densas).
- Cores, fonte e dimensões hoje são **constantes hardcoded** (`render_layered`, `WIDGET_HEIGHT`,
  `CreateFontW`, funções `*_accent_color`). As barras usam cor **baseada na usage** (tons quentes).
- **Não existe janela de configuração** hoje; toda config é pelo menu de clique-direito.
- `theme.rs` tem um tipo `Color` (`from_hex`, `to_colorref`). App é raw `windows` crate,
  binário ~0.8 MB — manter leve, **zero dependências novas**.

## 3. Decisões (do brainstorming)

| Tema | Decisão |
|---|---|
| Alvo de qualidade | Uso pessoal + publicação futura → código limpo, i18n (10 idiomas), polido |
| UI da tela | **Bespoke GDI**, tema escuro, on-brand, preview WYSIWYG (reusa o renderer) |
| Famílias de animação | **As 4**: fill eased/spring, shimmer/gloss, pulso-glow de alerta, fade/slide |
| Modelo de cor | **Paleta customizável mantendo semântica de usage** (RGBA em calm/atenção/crítico); fundo/texto/divisor RGBA fixo |
| Fonte | **Qualquer fonte instalada** + tamanho + peso, com preview |
| Aplicar/salvar | **Preview ao vivo na tela + Salvar/Cancelar/Resetar**; widget real só muda ao Salvar |
| Escopo | **Global** no V1, arquitetura pronta para override por-modelo depois |
| Presets | **Incluir no V1**: Padrão, Glass, Neon, Minimal |
| Frequência de update | **Nos dois lugares**: menu de clique-direito + tela (sincronizados) |
| Geometria | **Clamp automático** da altura à taskbar; largura livre até um máximo razoável |

## 4. Escopo

**No V1:**
- Janela de setup bespoke com navegação por seções + preview ao vivo fixo no topo.
- Cores: paleta de usage RGBA (calm/atenção/crítico) + fundo + texto + divisor RGBA + opacidade geral.
- Fonte: família (qualquer instalada) + tamanho + peso.
- Geometria: largura, espessura/altura, raio dos cantos, espessura das barras, espaçamento (com clamp).
- Animações (4 famílias), cada uma com toggle + parâmetros; toggle global "reduzir movimento".
- Frequência de update (presets 1/5/15/60 min + custom) sincronizada com o menu.
- 4 presets de estilo prontos.
- Salvar / Cancelar / Resetar padrões. Escopo global.
- i18n nos 10 idiomas.

**Fora do V1 (arquitetado para caber):**
- Override de cor/animação **por modelo** (Claude/Codex/Antigravity).
- Migração de Language / Models / Start-with-Windows / Updates para dentro da tela
  (continuam no menu de clique-direito).

## 5. Módulos e fronteiras

Novos arquivos:

- **`settings.rs`** — extrai o `SettingsFile` de dentro do `window.rs` e adiciona as novas
  seções. Responsável por schema, `version`, defaults (`#[serde(default)]`), migração,
  load/save do `settings.json`. Interface: `Settings` (struct em memória) + `load()` / `save()`.
- **`config_window.rs`** — a janela de setup: criação, layout, navegação de seções,
  roteamento de eventos, painel de preview, botões. Depende de `controls`, `settings`,
  `animation`, e do renderer (`paint_content`).
- **`controls.rs`** — controles GDI reutilizáveis: `Slider`, `RgbaPicker` (R/G/B/A + swatch +
  hex), `Dropdown` (fonte), `Segmented` (frequência/presets), `Toggle`. Cada um: desenha-se,
  processa hit-test/mouse, expõe valor atual via callback. Sem estado global.
- **`animation.rs`** — `AnimationClock` + estado por família + funções de easing. Interface:
  `tick(now) -> AnimationFrame` com valores interpolados (fill %, fase de shimmer, intensidade
  de glow, alpha de fade). Não conhece GDI.

Modificações:

- **`window.rs`** — item de menu "Configurações…" que abre `config_window`; `render_layered`/
  `paint_content` passam a **ler `Settings` + `AnimationFrame`** em vez de constantes; loop de
  timer de animação (ativa só quando há animação pendente).
- **`theme.rs`** — `Color` ganha componente **alpha** e conversões RGBA/hex `#RRGGBBAA`.

## 6. Modelo de dados (settings)

Esboço (nomes finais no plano). Tudo com default → migração indolor do `settings.json` atual.

```
Settings {
  version: u32,                     // p/ migração
  // ...campos existentes (tray_offset, taskbar_index, poll_interval_ms,
  //    language, widget_visible, show_claude_code, show_codex, show_antigravity)
  appearance: Appearance {
    palette: { calm: Rgba, attention: Rgba, critical: Rgba },  // usa semântica de usage
    background: Rgba,
    text: Rgba,
    divider: Rgba,
    opacity: f32,                   // 0..1, opacidade geral do widget
  },
  typography: { family: String, size_pt: f32, weight: Weight },
  geometry:  { width: i32, height: i32, corner_radius: i32,
               bar_thickness: i32, spacing: i32 },   // clamp aplicado no uso
  animation: {
    reduce_motion: bool,
    fill:       { on: bool, easing: Easing, speed: f32 },
    shimmer:    { on: bool, speed: f32, intensity: f32 },
    alert_glow: { on: bool, threshold: f32, intensity: f32 },
    fade_slide: { on: bool, duration_ms: u32 },
    preset: Option<PresetId>,       // último preset aplicado (informativo)
  },
}
```

`Rgba { r,g,b: u8, a: u8 }`. `Weight` = Regular|SemiBold|Bold. `Easing` = Cubic|Spring|Linear.

**Migração:** `version` ausente/antigo → assume V0, aplica defaults nas seções novas,
regrava com `version` atual. Nenhum campo existente muda de significado.

## 7. Motor de animação

- `AnimationClock` avança por timer (~60 fps) **apenas enquanto há animação ativa**; quando
  tudo assenta (fill chegou no alvo, sem shimmer/glow ligados, sem fade em curso) o timer para
  → 0% CPU em repouso.
- Estado por família:
  - **fill**: valor `%` corrente → alvo, com easing/spring; ao chegar, para.
  - **shimmer**: fase contínua (0..1) enquanto `on`; posição do gloss = f(fase).
  - **alert_glow**: intensidade pulsante quando `usage ≥ threshold`; senão 0.
  - **fade_slide**: alpha (e offset) em transições de mostrar/esconder/atualizar.
- `render_layered` chama `clock.tick(now)` e usa os valores; `paint_content` desenha com eles.
- `reduce_motion` = curto-circuita todas as famílias para o estado final estático.
- **Sem `Date.now` proibido** — usa `Instant`/`SystemTime` do runtime real do app (isto é código
  do app, não script de workflow).

## 8. Fluxo de dados

```
settings.json ⇄ Settings (global, atrás de Mutex/RwLock)
                     │
   render_layered ───┤ lê snapshot + AnimationFrame a cada frame → paint_content → DIB → UpdateLayeredWindow
                     │
config_window edita  ├─ DRAFT (cópia) ── preview usa o mesmo paint_content c/ o draft (WYSIWYG)
                     │
   [Salvar] ─────────┘ commit draft → global + escreve JSON + re-render/re-embed do widget real
   [Cancelar] descarta draft   [Resetar] draft ← defaults
```

## 9. Layout da janela

```
┌─────────────────────────────────────────────────────────────┐
│  ⚙  Configurações — Claude Code Usage Monitor            [x] │
├───────────────┬─────────────────────────────────────────────┤
│  Aparência    │   ┌─ PREVIEW AO VIVO (renderer real) ─────┐  │
│  Fonte        │   │   Claude  5h ▓▓▓▓▓▓░░░  62%           │  │
│  Tamanho      │   │           7d ▓▓▓▓░░░░░  41%           │  │
│  Animações    │   └──────────────────────────────────────┘  │
│  Atualização  │                                              │
│  Presets      │   [ controles da seção ativa ]              │
├───────────────┴─────────────────────────────────────────────┤
│                      [Resetar]      [Cancelar]   [Salvar]    │
└─────────────────────────────────────────────────────────────┘
```

- Preview fixo no topo do painel direito, atualiza a cada ajuste (inclui animações rodando).
- Abrir via item "Configurações…" no menu de clique-direito do widget e do tray.

## 10. Geometria e clamp

- Ao aplicar `geometry`, altura é limitada à altura útil da taskbar (`GetDpiForWindow`/rect da
  taskbar já usados hoje). Largura limitada a um máximo razoável. Ao Salvar, recomputa layout e
  re-embute o widget.

## 11. Tratamento de erros

- Fonte inválida/ausente → fallback para a fonte atual (Segoe UI).
- Geometria fora de faixa → clamp.
- Cor/hex inválidos → ignora edição, mantém valor anterior.
- `settings.json` corrompido/antigo → migra com defaults; nunca derruba o app.

## 12. Testes

- **Unit:** (de)serialização + migração de `Settings`; math de easing (cubic/spring/linear);
  conversões `Rgba`↔`#RRGGBBAA`; clamp de geometria; interpolação da paleta por % de usage.
- **UI:** validada pelo preview + manual; smoke test criando a janela offscreen (não deve panicar).
- Rodar via `.\dev.ps1` (build release/debug) e inspecionar o widget + a tela.

## 13. i18n

- Novas strings de UI adicionadas ao mecanismo `strings()` em todos os 10 locales
  (`english.rs`, `portuguese_brazil.rs`, …). Nomes de fonte não são traduzidos.

## 14. Fora de escopo / futuro

- Override por-modelo (abas por provider).
- Centralizar Language/Models/Startup/Updates na tela.
- Mais presets / import-export de tema.
