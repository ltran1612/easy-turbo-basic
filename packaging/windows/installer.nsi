; Easy Turbo Basic — Windows installer.
;
;
; This file begins with a UTF-8 byte order mark, and must keep it.
;
; `Unicode true` above makes the *installer* Unicode; it says nothing about
; how makensis reads this source. Without the mark, makensis reads the file in
; the build machine's ANSI codepage — so the Vietnamese below survives a build
; on a UTF-8 Linux box and comes out doubly encoded when CI builds it on
; Windows, where the codepage is CP1252. That is how `Gỡ cài đặt` reached a
; Start Menu as `Gá»¡ cÃ i Ä‘áº·t`: mojibake in the first screen a
; Vietnamese-speaking user ever sees, and in the shortcut left behind.
; Built by CI with makensis over a staged package directory, i.e. the output of
; `cargo xtask package --target windows-x86_64`.
;
;   makensis -DSRC=<staged dir> -DVERSION=<x.y.z> -DICON=<icon.ico> \
;            -DOUT=<file.exe> installer.nsi
;
; Two decisions worth stating, because both are deliberate:
;
;   * Per-user install, so there is no UAC prompt. An elevation dialog on an
;     unsigned installer is where a non-technical user stops, and that costs more
;     than the marginal benefit of an admin-only install directory. The toolchain
;     integrity manifest recovers most of that anyway.
;
;   * The interface is English (NSIS's own), but every string this file supplies
;     is Vietnamese first. The person installing this is Vietnamese-first, and
;     the application itself is fully bilingual.

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef SRC
  !error "SRC must be defined: the staged package directory"
!endif
; Absolute, and required rather than defaulted: `${__FILEDIR__}` resolves
; differently depending on where makensis was invoked from.
!ifndef ICON
  !error "ICON must be defined: the path to icon.ico (cargo xtask gen-icons)"
!endif
!ifndef OUT
  !define OUT "EasyTurboBasic-Setup.exe"
!endif

!define APPNAME "Easy Turbo Basic"
!define COMPANY "Easy Turbo Basic"
!define REGKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\EasyTurboBasic"

Name "${APPNAME} ${VERSION}"
OutFile "${OUT}"
; Per-user: no UAC prompt, and no admin rights needed.
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\Easy Turbo Basic"
InstallDirRegKey HKCU "Software\EasyTurboBasic" "InstallDir"
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUnInstDetails show

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APPNAME}"
VIAddVersionKey "FileDescription" "${APPNAME} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT OR Apache-2.0; bundled GNU toolchain under GPLv3"

; The icon on Setup.exe is the first thing the user sees, before anything is
; installed -- it is on the file they download. Same artwork as the application
; and the uninstaller, generated from logo.png by `cargo xtask gen-icons`.
;
!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "${APPNAME}"
!define MUI_WELCOMEPAGE_TEXT "Chương trình này giúp bạn chạy các chương trình Turbo Basic trên Windows ngày nay, mà không cần dùng dòng lệnh.$\r$\n$\r$\nThis installs ${APPNAME}, which builds Turbo Basic programs into programs that run on today's Windows, without needing a command line.$\r$\n$\r$\nNó sẽ được cài vào thư mục cá nhân của bạn, nên không cần quyền quản trị.$\r$\nIt installs into your own user folder, so no administrator rights are needed."

; The GPL applies to the bundled compiler, so the text travels with it.
!insertmacro MUI_PAGE_WELCOME
!define MUI_LICENSEPAGE_TEXT_TOP "Trình biên dịch đi kèm dùng giấy phép GPL phiên bản 3."
!define MUI_LICENSEPAGE_BUTTON "Tiếp tục / Continue"
!define MUI_LICENSEPAGE_TEXT_BOTTOM "Chương trình bạn tự biên dịch KHÔNG bị ràng buộc bởi giấy phép này.$\r$\nPrograms you compile yourself are NOT covered by it."
!insertmacro MUI_PAGE_LICENSE "${SRC}\LICENSES\gpl-3.0.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\easy-turbo-basic.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Mở ${APPNAME} ngay bây giờ / Open ${APPNAME} now"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  ; The whole staged package: the application, the toolchain beside it, and the
  ; licences. The application looks for the compiler at <exe>\..\toolchain.
  File /r "${SRC}\*.*"

  WriteRegStr HKCU "Software\EasyTurboBasic" "InstallDir" "$INSTDIR"
  CreateDirectory "$SMPROGRAMS\${APPNAME}"
  CreateShortCut "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk" "$INSTDIR\easy-turbo-basic.exe"
  CreateShortCut "$SMPROGRAMS\${APPNAME}\Gỡ cài đặt - Uninstall.lnk" "$INSTDIR\Uninstall.exe"

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegStr   HKCU "${REGKEY}" "DisplayName"     "${APPNAME}"
  WriteRegStr   HKCU "${REGKEY}" "DisplayVersion"  "${VERSION}"
  WriteRegStr   HKCU "${REGKEY}" "Publisher"       "${COMPANY}"
  WriteRegStr   HKCU "${REGKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr   HKCU "${REGKEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${REGKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${REGKEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${REGKEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  ; First, what the application made for itself outside this directory: the
  ; work trees, and — on a machine whose own path the compiler cannot be given — its
  ; copy of the compiler, which is most of a gigabyte. The application knows
  ; where those are and this does not, so it is asked. A failure here is not
  ; worth stopping an uninstall over.
  IfFileExists "$INSTDIR\easy-turbo-basic.exe" 0 +2
    ExecWait '"$INSTDIR\easy-turbo-basic.exe" --clear-scratch'

  ; Then only what was installed. The user's own programs are never here,
  ; and the user's saved programs live in AppData, which is left alone: uninstalling
  ; should not throw away the user's list of programs.
  Delete "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk"
  Delete "$SMPROGRAMS\${APPNAME}\Gỡ cài đặt - Uninstall.lnk"
  RMDir  "$SMPROGRAMS\${APPNAME}"

  Delete "$INSTDIR\easy-turbo-basic.exe"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir /r "$INSTDIR\toolchain"
  RMDir /r "$INSTDIR\LICENSES"
  RMDir /r "$INSTDIR\examples"
  ; Non-recursive on purpose: it removes the directory only once nothing is left
  ; in it. So everything installed has to be named above, or this quietly fails
  ; and the install directory survives the uninstall.
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${REGKEY}"
  DeleteRegKey HKCU "Software\EasyTurboBasic"
SectionEnd
