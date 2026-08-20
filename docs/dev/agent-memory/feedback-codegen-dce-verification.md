---
name: feedback-codegen-dce-verification
description: "проверка codegen-изменений (DCE/эмиссия) — baseline = kill-switch на ТОМ ЖЕ бинаре, не sibling-бинарь; гонять ПОЛНЫЙ регресс"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1e103df6-8d3d-4ee9-80ac-d67a5be852c1
---

При верификации codegen-изменений, влияющих на то ЧТО эмитится в C (DCE/reachability, элизия, mono), две вещи обязательны:

1. **Baseline = kill-switch на ТОМ ЖЕ бинаре**, а не другой (sibling/main) бинарь. На Plan 159 я сравнил ветку с релизным бинарём main-репы — он оказался **устаревшим** (собран в 15:55, а main ушёл вперёд через мёржи других агентов). Сравнение «ветка-бинарь на фикстурах ветки» vs «устаревший main-бинарь на фикстурах main» меняет ДВА параметра → ветка ложно выглядела «сильно лучше main» (на 33 теста в plan114_4_4) — артефакт устаревшего бинаря. Единственный confound-free baseline: флаг-переключатель (напр. `NOVA_REACH_DCE=0`) на **том же** бинаре и **тех же** фикстурах — отличается только проверяемый флаг.

2. **Гонять ПОЛНЫЙ регресс, не сэмпл**, перед мёржем codegen-изменения. Воркфлоу-сэмпл был зелёный, но полный прогон поймал реальный over-prune: интерполяция в сообщениях контрактов (`requires/ensures/invariant "...${f}..."`) — `collect_used_names` обходил `Contract.expr`, но не `Contract.message_expr` → method-DCE срезал int→str-конвертер. Это эмпирическая иллюстрация хрупкости coarse-by-name method-DCE: список засеянных desugar-селекторов собирается регрессией, а не систематическим перечнем.

3. **Флака ≠ регрессия:** stress/timeout-тест (plan103_5 once_stress) мелькнул как FAIL, повторный прогон — PASS. Не блокер.

**Why:** устаревший baseline-бинарь делает гейт бессмысленным (всё «проходит»); сэмпл пропускает over-prune, который роняет прод.
**How to apply:** для любого изменения эмиссии — kill-switch ON/OFF на одном бинаре per падавший dir + полный батчевый регресс (см. [[project-bash-timeout-10min-max]] — дробить на батчи <10мин). Связано: [[feedback-no-interpreter]] (тест только через C-codegen), [[project-nova-test-vs-test-build]].
