@echo off
echo === find-anything: initial scan ===
echo This will index all configured directories. Please wait...
echo.
"%~dp0find-scan.exe" --config "%~dp0client.toml" --full
echo.
echo === Starting find-watch service ===
sc start FindAnythingWatcher
echo.
echo === Starting system tray icon ===
start "" "%~dp0find-tray.exe" --config "%~dp0client.toml"
echo.
echo Done. find-watch is now running in the background.
pause
