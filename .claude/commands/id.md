---
description: показать id текущей сессии и её файл истории
allowed-tools: Bash
---

Покажи id ЭТОЙ сессии. Коротко, ничего не меняя.

Возьми его машинно, из окружения — не из памяти и **не по времени файлов**: рядом
идут параллельные окна того же проекта, и по mtime легко назвать чужую сессию.
Одна команда, целиком:

```sh
CFG=$(cygpath -u "$CLAUDE_CONFIG_DIR" 2>/dev/null || echo "$CLAUDE_CONFIG_DIR")
F=$(ls "$CFG"/projects/*/"$CLAUDE_CODE_SESSION_ID".jsonl 2>/dev/null)
echo "id=$CLAUDE_CODE_SESSION_ID"; echo "file=$F"; ls -l "$F" | awk '{print "bytes="$5}'; echo "lines=$(wc -l < "$F")"
```

Ответ — три строки: id, путь к файлу истории, размер и число записей.

Если `CLAUDE_CODE_SESSION_ID` пуст — так и скажи: «окружение id не отдало».
Не подставляй вместо него самый свежий файл в `projects/` — это будет чужая
сессия, и ошибка молчаливая.
