; Rover Browser 2.0 — Native Windows NSIS Modern UI 2 Installer
; Pure Open-Source Chromium 156 (CEF) + Rust — Zero Microsoft Edge / WebView2.

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"

!define APP_NAME "Rover"
!define APP_FULL_NAME "Rover Browser"
!define APP_VERSION "2.0.1"
!define APP_PUBLISHER "Rover Project"
!define APP_EXE "rover.exe"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Rover"

Name "${APP_FULL_NAME} ${APP_VERSION}"
OutFile "..\..\dist\RoverSetup-${APP_VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME}"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor zlib

; Custom Inverted Mars Rover Icons & Installer Graphics
!define MUI_ICON "rover.ico"
!define MUI_UNICON "rover.ico"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "installer_header.bmp"
!define MUI_HEADERIMAGE_RIGHT
!define MUI_WELCOMEFINISHPAGE_BITMAP "installer_sidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "installer_sidebar.bmp"
!define MUI_ABORTWARNING

; Welcome Page Copy
!define MUI_WELCOMEPAGE_TITLE "Welcome to Rover ${APP_VERSION} Setup"
!define MUI_WELCOMEPAGE_TEXT "This wizard will install Rover — a pure Chromium-based browser rebuilt in Rust with a built-in low-token AI agent.$\r$\n$\r$\n• Pure open-source Chromium 156 engine (libcef.dll) with docked DevTools$\r$\n• Session tab persistence, live omnibox & search autocomplete$\r$\n• 120× faster Aho-Corasick ad & tracker blocker (2.00 µs/req)$\r$\n• Up to 86.9% fewer AI tokens via DOM Delta Snapshots & Compact DSL$\r$\n$\r$\nClick Next to continue."

; Finish Page Launch Checkbox
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ${APP_FULL_NAME}"
!define MUI_FINISHPAGE_LINK "Open Rover Documentation (README)"
!define MUI_FINISHPAGE_LINK_LOCATION "$INSTDIR\README.md"

; Installer Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; PE Version Info for RoverSetup-2.0.1.exe
VIProductVersion "2.0.1.0"
VIAddVersionKey "ProductName" "${APP_FULL_NAME}"
VIAddVersionKey "CompanyName" "${APP_PUBLISHER}"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 ${APP_PUBLISHER}"
VIAddVersionKey "FileDescription" "${APP_FULL_NAME} ${APP_VERSION} Installer (Pure Chromium CEF)"
VIAddVersionKey "FileVersion" "2.0.1.0"
VIAddVersionKey "ProductVersion" "2.0.1.0"

Section "Rover Browser Core" SecCore
  SectionIn RO
  SetOutPath "$INSTDIR"

  ; Main Windows 64-bit Pure Chromium 156 (CEF) Browser + Runtime (libcef.dll, chrome_elf.dll, *.pak, locales\)
  File /r "..\..\dist\stage\*.*"

  ; Assets & Branding
  SetOutPath "$INSTDIR\assets"
  File "..\..\assets\rover.ico"
  File "..\..\assets\rover-icon.png"
  File "..\..\assets\rover.svg"
  File "..\..\assets\darkmode.user.js"

  ; Documentation
  SetOutPath "$INSTDIR"
  File "..\..\README.md"
  File "..\..\FEATURES.md"
  File "..\..\CHANGELOG.md"

  ; Start Menu & Desktop Shortcuts with Custom Inverted Mars Rover Icon
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_FULL_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\assets\rover.ico" 0
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\Rover Benchmarks.lnk" "cmd.exe" '/K "$INSTDIR\${APP_EXE}" --bench' "$INSTDIR\assets\rover.ico" 0
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\assets\rover.ico" 0
  CreateShortcut "$DESKTOP\${APP_FULL_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\assets\rover.ico" 0

  ; Register rover: URL Protocol Handler
  WriteRegStr HKCU "Software\Classes\rover" "" "URL:Rover Protocol"
  WriteRegStr HKCU "Software\Classes\rover" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\rover\DefaultIcon" "" "$INSTDIR\assets\rover.ico,0"
  WriteRegStr HKCU "Software\Classes\rover\shell\open\command" "" '"$INSTDIR\${APP_EXE}" "%1"'

  ; Write Uninstaller & Add/Remove Programs Metadata
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_FULL_NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\assets\rover.ico"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\${APP_FULL_NAME}.lnk"
  RMDir /r "$SMPROGRAMS\${APP_NAME}"
  RMDir /r "$INSTDIR\locales"
  RMDir /r "$INSTDIR\assets"
  Delete "$INSTDIR\*.dll"
  Delete "$INSTDIR\*.pak"
  Delete "$INSTDIR\*.dat"
  Delete "$INSTDIR\*.bin"
  Delete "$INSTDIR\*.json"
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\FEATURES.md"
  Delete "$INSTDIR\CHANGELOG.md"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Classes\rover"
SectionEnd
