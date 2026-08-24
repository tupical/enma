# Enma 閻魔 — decisions-слой Meisei

> **Meisei** 明晰 («ясность») — открытый конвейер, который проводит сырой замысел
> через понимание → решение → план → действие к готовому результату.

[![Meisei](https://img.shields.io/badge/meisei-明晰-1f2937.svg)](https://meisei.ru)
[![License: Apache-2.0 WITH Commons-Clause](https://img.shields.io/badge/license-Apache--2.0%20WITH%20Commons--Clause-blue.svg)](LICENSE)

<sub>
torii · satori · <b>enma</b> · yatagarasu · fujin · daruma
&nbsp;—&nbsp; intake · осмысление · <b>решения</b> · планирование · действия · исполнение (терминальный слой)
</sub>

## Что это

Enma — **decisions**-слой конвейера MeiSei: превращает понимание в
*направление*. Владеет примитивами цели и решения — `Decision` (зафиксированный
выбор: формулировка, автор, обоснование, рассмотренные `Alternative`,
принятые последствия и условия пересмотра) и `Directive` (шесть более лёгких
примитивов направления: `goal`, `non_goal`, `constraint`, `assumption`,
`principle`, `success_criteria`). Решения и директивы ссылаются назад на
осмысление, на котором они стоят, и вперёд — на планирование, которое они
направляют. Decisions **не исполняет**: ничего здесь не планируется, не
запускается и не мутирует план — реализация выбора лежит на Planning и
Actions. Крейт не зависит от daruma и соседних слоёв; адаптеры живут только
внутри host.

## Структура репозитория

- `src/` — библиотека `enma`: примитивы `Decision`/`Directive`, связи,
  `decide_ai`, типы ошибок.
- `server/` — `enma-server`, тонкая независимо развёртываемая HTTP/MCP-обёртка
  над библиотекой (axum/tokio-каркас — из [`layer-kit`](../layer-kit)).
- `deploy/` — release-`build.sh` (прошивает git SHA в `/healthz`) и systemd user unit.

## Сборка и запуск

```sh
cargo run -p enma-server
# GET  /healthz   — открытая проба живости/версии
# POST /v1/mcp    — MCP-поверхность под платформенным токеном:
#                   enma.decide; read-методы: enma.list /
#                   enma.list_decisions, enma.get / enma.get_decision
```

Для продовых сборок используйте `deploy/build.sh`, чтобы `/healthz` отдавал
реальный git SHA, а не `"dev"`.

## Конфигурация (env)

| Переменная | По умолчанию | Назначение |
| --- | --- | --- |
| `ENMA_PORT` | `8092` | HTTP-порт |
| `ENMA_PLATFORM_SECRET` | не задан | HMAC-ключ; если не задан, `/v1/mcp` закрыт |
| `ENMA_VERSION` | версия крейта | Версия, отдаваемая `/healthz` |
| `ENMA_DB` | `./enma.db` | Путь к SQLite-хранилищу (`layer_kit::store::Store`) |
| `OPENAI_API_KEY` | не задан | Опциональный AI-fallback для `enma.decide`; без ключа метод отвечает 503 `ai_not_configured` |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Базовый URL OpenAI-совместимого API |
| `OPENAI_MODEL` | `gpt-4.1` | Модель, используемая AI-fallback'ом |

## Документация

Канон конвейера и контракты слоёв: https://meisei.ru/docs

## Лицензия

Apache-2.0 WITH Commons-Clause — см. [LICENSE](LICENSE) и
[LICENSE.commons-clause.md](LICENSE.commons-clause.md).
