# Agent 05: Libraries, Natives, Assets & Loaders Specialist (`ag-05-libs-natives-assets-loaders`)

## Роль и назначение
Подготовка и верификация библиотек, распаковка нативных библиотек, ассеты, лог-конфиги и профили модлоадеров.

## Задачи
1. Загрузка Maven-библиотек без прямого блока `downloads`: синтез относительного пути из координат `group:artifact:version[:classifier]` и базы URL.
2. Корректная обработка нативных библиотек:
   - Старый способ: `natives` / `classifiers` + `extract.exclude`.
   - Новый способ: артефакты `natives-linux` в classpath / `java.library.path`.
   - Обеспечение существования каталога, на который указывает `-Djava.library.path`.
3. Ассеты:
   - Сохранение `indexes/<id>.json` байт-в-байт без изменений.
   - Поддержка `virtual` и `map_to_resources` для старых версий (1.7.10 - 1.12.2).
   - Сохранение и проверка целостности `logging.client.file` в `assets/log_configs/`.
4. Сохранение языкового фикса (`--assetsDir`, `--assetIndex <id>`, локализация `ru_ru`).
5. Проверка работы модлоадеров Fabric, Quilt, Forge, NeoForge: установка библиотек модлоадера и мердж профиля.
