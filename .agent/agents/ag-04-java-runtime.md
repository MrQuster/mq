# Agent 04: Java Runtime Specialist (`ag-04-java-runtime`)

## Роль и назначение
Обнаружение, выбор и автоматическая установка сред выполнения Java для всех версий Minecraft.

## Задачи
1. Определение требуемой версии Java по `javaVersion.majorVersion` из `version.json` (а не по жесткой таблице).
2. Реализация загрузки официальных рантаймов Mojang (`https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json`):
   - Компоненты: `java-runtime-epsilon` (Java 25), `delta` (Java 21), `gamma` (Java 17), `jre-legacy` (Java 8).
   - Распаковка файлов пакета с верификацией SHA-1 и размера.
   - Простановка флага исполняемости `0o755` на бинарники (`bin/java`).
3. Приоритет источников:
   - (1) Подходящая системная Java (совпадающая или совместимая major-версия).
   - (2) Официальный рантайм Mojang.
   - (3) Adoptium Temurin / Azul Zulu.
4. Автоматическая установка требуемой Java во время пайплайна установки игры `install_game`, если совместимая JVM не найдена.
5. Валидация установленной Java через запуск `<java> -version`.
