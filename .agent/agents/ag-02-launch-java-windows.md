# Agent 02: Game Launch & Java on Windows

- **Zone:** `crates/mq-launch/`
- **Ownership:** Launch command generation, Windows JVM arguments, classpath separator `;`, natives `natives-windows`, process spawning with `CREATE_NO_WINDOW`, process tree killing (`taskkill /T`), Adoptium Java discovery and downloading for Windows x64.
- **Goals:**
  1. Classpath separator: use `;` on Windows, `:` on Linux.
  2. Rule parsing: evaluate `rules` in `version.json` for `os.name = "windows"`, `os.arch`, `os.version`.
  3. Extract `natives-windows` archives.
  4. Process execution: spawn `java.exe` using `Command::new(..).args(..)`. On Windows, set creation flags `CREATE_NO_WINDOW` (0x08000000) so no extraneous cmd console window opens, while piping stdout/stderr into the launcher's log stream.
  5. Process termination: terminate game with full process tree (`taskkill /F /T /PID` or Job Object without kill-on-close).
  6. Hide Linux wrappers (`gamemoderun`, `mangohud`, `prime-run`) on Windows.
  7. Java discovery on Windows: check `JAVA_HOME`, Windows Registry (`HKLM\SOFTWARE\JavaSoft` & `HKLM\SOFTWARE\Eclipse Adoptium`), `Program Files`.
  8. Adoptium JRE auto-download: download Windows x64 `.zip` from Adoptium API, verify SHA-256, unpack into `<java_dir>/temurin-<ver>-x64`.
