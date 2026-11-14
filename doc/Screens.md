ich starte das program mit:
target/release/gentoo-dep-tui  --binhost /tmp/Packages

```
 [1] Pakete & Alternativen │ [2] Global Big Levers

──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
┌Pakete (587)─────────────────────────────────┐┌Details / USE-Flags───────────────────────────────────────────┐┌Blame / Pros-Cons────────────────────────────┐
│> sys-kernel/gentoo-sources                  ││sys-kernel/gentoo-sources  Binpkg: kein Binpkg (Source-Build) ││Warum ist das hier?                          │
│  app-admin/sudo                             ││                                                              ││(kein Pfad — Welt-Paket oder leer)           │
│  app-admin/sysklogd                         ││[ ] build -6.8 MB bin / -46.2 MB dl (+0/-11 pkg)              ││                                             │
│  app-arch/dpkg                              ││[ ] experimental 0 B bin / 0 B dl                             ││Einschalten von build bewirkt:               │
│  app-containers/docker                      ││[ ] symlink 0 B bin / 0 B dl                                  ││- app-alternatives/bc                        │
│  app-containers/docker-buildx               ││[ ] vanilla 0 B bin / 0 B dl                                  ││- app-alternatives/lex                       │
│  app-containers/docker-cli                  ││                                                              ││- app-alternatives/yacc                      │
│  app-crypt/p11-kit                          ││                                                              ││- dev-build/make                             │
│  app-editors/emacs                          ││                                                              ││- sys-devel/bc                               │
│  app-misc/fastfetch                         ││                                                              ││- sys-devel/binutils-config                  │
│  app-misc/fdupes                            ││                                                              ││- sys-devel/bison                            │
│  app-misc/jq                                ││                                                              ││- sys-devel/flex                             │
│  app-misc/mc                                ││                                                              ││(...)                                        │
│  app-misc/tmate                             ││                                                              ││                                             │
│  app-misc/tmux                              ││                                                              ││Geteilte Abhaengigkeiten:                    │
│  app-pda/ifuse                              ││                                                              ││app-alternatives/bc                          │
│  app-portage/cpuid2cpuflags                 ││                                                              ││gehalten von sys-devel/bc                    │
│  app-portage/eix                            ││                                                              ││app-alternatives/cpio                        │
│  app-portage/gentoolkit                     ││                                                              ││gehalten von app-arch/cpio                   │
│  app-shells/bash-completion                 ││                                                              ││gehalten von sys-kernel/dracut               │
│  app-shells/gentoo-zsh-completions          ││                                                              ││dev-lang/perl                                │
│  app-shells/zsh                             ││                                                              ││gehalten von app-admin/perl-cleaner          │
│  app-text/mupdf                             ││                                                              ││gehalten von app-arch/dpkg                   │
│  app-text/tree                              ││                                                              ││gehalten von dev-perl/Authen-SASL            │
│  dev-build/ninja                            ││                                                              ││gehalten von dev-perl/Crypt-URandom          │
│  dev-debug/gdb                              ││                                                              ││sys-devel/bison                              │
│  dev-debug/ltrace                           ││                                                              ││gehalten von app-alternatives/yacc           │
│  dev-debug/strace                           ││                                                              ││sys-devel/flex                               │
│  dev-libs/nss                               ││                                                              ││gehalten von app-alternatives/lex            │
│  dev-lisp/sbcl                              ││                                                              ││sys-libs/ncurses                             │
│  dev-python/btrfs                           ││                                                              ││gehalten von app-admin/eselect               │
│  dev-python/uv                              ││                                                              ││gehalten von app-arch/dpkg                   │
│  dev-vcs/git                                ││                                                              ││gehalten von app-editors/emacs               │
│  media-fonts/wqy-zenhei                     ││                                                              ││gehalten von app-editors/nano                │
│  media-gfx/feh                              ││                                                              ││virtual/libelf                               │
│  media-gfx/scrot                            ││                                                              ││gehalten von dev-debug/ltrace                │
│  media-libs/fontconfig                      ││                                                              ││gehalten von dev-libs/glib                   │
│  media-sound/pulsemixer                     ││                                                              ││virtual/pkgconfig                            │
│  media-video/mpv                            ││                                                              ││gehalten von dev-libs/gobject-introspection  │
│  media-video/pipewire                       ││                                                              ││gehalten von dev-libs/nss                    │
│  media-video/wireplumber                    ││                                                              ││gehalten von sys-kernel/dracut               │
│  net-analyzer/hping                         ││                                                              ││                                             │
│  net-analyzer/iftop                         ││                                                              ││build: !!internal use only!! DO NOT SET THIS │
└─────────────────────────────────────────────┘└──────────────────────────────────────────────────────────────┘└─────────────────────────────────────────────┘
──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 756.6 MB (503/554 Hits)  Quell-DL 3313.0 MB  Build-Only 243.2 MB bin / 1761.3 MB dl  Pakete 554  Konflikte 0
[Space] Toggle  [Tab] Pane  [1/2] View  [/] Suche  [o] Sort  [x] Cut  [a] Apply  [u] Undo  [S] package.use  [?] Hilfe  [q] Quit
```

ich sortiere linke spalte nach groesse mit o
```
┌Pakete (587)──────────────────────────────────┐┌Details / USE-Flags──────────────────────────────────────────┐┌Blame / Pros-Cons─────────────────────────────┐
│> sys-power/tlp (19 pkg, 11.7 MB)             ││sys-power/tlp  Binpkg: kein Binpkg (Source-Build)  Pkg-Quell-││Warum ist das hier?                           │
│  dev-vcs/git (13 pkg, 10.6 MB)               ││                                                             ││(kein Pfad — Welt-Paket oder leer)            │
│  sys-kernel/gentoo-sources (12 pkg, 6.8 MB)  ││[x] ppd -800.0 kB bin / -2.2 MB dl (+0/-3 pkg)               ││                                              │
│  x11-terms/xterm (10 pkg, 6.3 MB)            ││[ ] python_single_target_python3_12 0 B bin / 0 B dl CONFLICT││Ausschalten von ppd bewirkt:                  │
│  media-video/mpv (9 pkg, 4.9 MB)             ││[ ] python_single_target_python3_13 0 B bin / 0 B dl CONFLICT││- dev-python/dbus-python                      │
│  app-containers/docker (8 pkg, 53.3 MB)      ││[x] python_single_target_python3_14 0 B bin / 0 B dl         ││- dev-python/pycairo                          │
│  net-analyzer/wireshark (7 pkg, 53.1 MB)     ││[x] rdw -10.8 MB bin / -17.6 MB dl (+0/-14 pkg)              ││- dev-python/pygobject                        │
│  x11-base/xorg-server (7 pkg, 3.7 MB)        ││                                                             ││                                              │
│  app-pda/ifuse (7 pkg, 430.0 kB)             ││                                                             ││Geteilte Abhaengigkeiten:                     │
│  app-text/mupdf (6 pkg, 27.8 MB)             ││                                                             ││dev-lang/python                               │
│  app-editors/emacs (6 pkg, 35.5 MB)          ││                                                             ││gehalten von app-accessibility/at-spi2-core   │
│  net-misc/mosh (5 pkg, 6.5 MB)               ││                                                             ││gehalten von app-portage/gemato               │
│  sys-apps/qdirstat (5 pkg, 170.0 kB)         ││                                                             ││gehalten von app-portage/gentoolkit           │
│  net-dns/bind (5 pkg, 2.3 MB)                ││                                                             ││gehalten von dev-debug/gdb                    │
│  sys-devel/mold (4 pkg, 2.9 MB)              ││                                                             ││                                              │
│  dev-lisp/sbcl (4 pkg, 310.0 kB)             ││                                                             ││ppd: TLP Profiles Daemon that replaces        │
│  app-admin/sudo (4 pkg, 2.4 MB)              ││                                                             ││power-profiles-daemon by using the same D-Bus │
│  net-analyzer/ngrep (4 pkg, 850.0 kB)        ││                                                             ││API.                                          │
│  net-misc/chrony (4 pkg, 470.0 kB)           ││                                                             ││                                              │
└──────────────────────────────────────────────┘└─────────────────────────────────────────────────────────────┘└──────────────────────────────────────────────┘
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 756.6 MB (503/554 Hits)  Quell-DL 3313.0 MB  Build-Only 243.2 MB bin / 1761.3 MB dl  Pakete 554  Konflikte 0
[Space] Toggle  [Tab] Pane  [1/2] View  [/] Suche  [o] Sort  [x] Cut  [a] Apply  [u] Undo  [S] package.use  [?] Hilfe  [q] Quit
```
ich schalte rdw, ppd und python aus und druecke S um zu speichern
```
┌Pakete (569)──────────────────────────────────┐┌Details / USE-Flags──────────────────────────────────────────┐┌Blame / Pros-Cons─────────────────────────────┐
│  app-misc/tmate (3 pkg, 300.0 kB)            ││sys-power/tlp  Binpkg: kein Binpkg (Source-Build)  Pkg-Quell-││Warum ist das hier?                           │
│  media-video/wireplumber (3 pkg, 780.0 kB)   ││                                                             ││(kein Pfad — Welt-Paket oder leer)            │
│  app-portage/eix (3 pkg, 820.0 kB)           ││[ ] ppd +800.0 kB bin / +2.2 MB dl (+3/-0 pkg)               ││                                              │
│  net-dns/dnsmasq (3 pkg, 570.0 kB)           ││[ ] python_single_target_python3_12 0 B bin / 0 B dl         ││Einschalten von rdw bewirkt:                  │
│  net-im/signal-desktop-bin (2 pkg, 11.6 MB)  ││[ ] python_single_target_python3_13 0 B bin / 0 B dl         ││+ dev-libs/jansson                            │
│  net-vpn/tailscale (2 pkg, 400.0 kB)         ││[ ] python_single_target_python3_14 0 B bin / 0 B dl         ││+ dev-libs/libgudev                           │
│  net-misc/freerdp (2 pkg, 20.0 MB)           ││[ ] rdw +10.8 MB bin / +17.6 MB dl (+14/-0 pkg)              ││+ dev-libs/newt                               │
│  sys-fs/cryptsetup (2 pkg, 1.4 MB)           ││                                                             ││+ net-dialup/ppp                              │
│  net-misc/tigervnc (2 pkg, 6.1 MB)           ││                                                             ││+ net-dialup/ppp-scripts                      │
│  app-misc/mc (2 pkg, 2.5 MB)                 ││                                                             ││+ net-libs/libmbim                            │
│  app-misc/fastfetch (2 pkg, 170.0 kB)        ││                                                             ││+ net-libs/libndp                             │
│  sys-fs/lvm2 (2 pkg, 500.0 kB)               ││                                                             ││+ net-libs/libqmi                             │
│  app-misc/jq (2 pkg, 270.0 kB)               ││                                                             ││(...)                                         │
│  media-video/pipewire (2 pkg, 2.6 MB)        ││                                                             ││                                              │
│  sys-apps/nvme-cli (2 pkg, 1.2 MB)           ││                                                             ││rdw: Enables TLP Radio Device Wizard feature  │
│  sys-kernel/dracut (2 pkg, 180.0 kB)         ││                                                             ││                                              │
│  sys-fs/erofs-utils (2 pkg, 350.0 kB)        ││                                                             ││                                              │
│> sys-power/tlp (2 pkg, 120.0 kB)             ││                                                             ││                                              │
└──────────────────────────────────────────────┘└─────────────────────────────────────────────────────────────┘└──────────────────────────────────────────────┘
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 745.0 MB (486/537 Hits)  Quell-DL 3293.2 MB  Build-Only 243.2 MB bin / 1761.3 MB dl  Pakete 537  Konflikte 0
[Space] Toggle  [Tab] Pane  [1/2] View  [/] Suche  [o] Sort  [x] Cut  [a] Apply  [u] Undo  [S] package.use  [?] Hilfe  [q] Quit  package.use geschrieben:
/tmp/gentoo-dep-tui.package.use
```
kiel@localhost ~ $ cat /tmp/gentoo-dep-tui.package.use 
sys-power/tlp -ppd -python_single_target_python3_14 -rdw

jetzt ist git ziemlich gross:
```
┌Pakete (569)──────────────────────────────────┐┌Details / USE-Flags──────────────────────────────────────────┐┌Blame / Pros-Cons─────────────────────────────┐
│> dev-vcs/git (13 pkg, 10.6 MB)               ││dev-vcs/git  Binpkg: 9.8 MB  Pkg-Quell-DL: 8.7 MB            ││Warum ist das hier?                           │
│  sys-kernel/gentoo-sources (12 pkg, 6.8 MB)  ││                                                             ││Kette 1:                                      │
│  x11-terms/xterm (10 pkg, 6.3 MB)            ││[ ] cgi +780.0 kB bin / +689.8 kB dl (+19/-0 pkg)            ││app-containers/docker -> dev-vcs/git          │
│  media-video/mpv (9 pkg, 4.9 MB)             ││[x] curl 0 B bin / 0 B dl CONFLICT                           ││                                              │
│  app-containers/docker (8 pkg, 53.3 MB)      ││[ ] cvs +3.2 MB bin / +3.5 MB dl (+8/-0 pkg)                 ││Einschalten von cgi bewirkt:                  │
│  app-pda/ifuse (8 pkg, 450.0 kB)             ││[ ] debug 0 B bin / 0 B dl                                   ││+ dev-perl/CGI                                │
│  net-analyzer/wireshark (7 pkg, 53.1 MB)     ││[ ] doc 0 B bin / +1.7 MB dl                                 ││+ dev-perl/Clone                              │
│  x11-base/xorg-server (7 pkg, 3.7 MB)        ││[x] gpg 0 B bin / 0 B dl                                     ││+ dev-perl/Encode-Locale                      │
│  app-text/mupdf (6 pkg, 27.8 MB)             ││[ ] highlight 0 B bin / 0 B dl                               ││+ dev-perl/HTML-Parser                        │
│  app-editors/emacs (6 pkg, 35.5 MB)          ││[x] iconv 0 B bin / 0 B dl                                   ││+ dev-perl/HTML-Tagset                        │
│  net-misc/mosh (5 pkg, 6.5 MB)               ││[ ] keyring +1.7 MB bin / +2.0 MB dl (+6/-0 pkg)             ││+ dev-perl/HTTP-Date                          │
│  sys-apps/qdirstat (5 pkg, 170.0 kB)         ││[x] nls 0 B bin / 0 B dl                                     ││+ dev-perl/HTTP-Message                       │
│  net-dns/bind (5 pkg, 2.3 MB)                ││[x] pcre 0 B bin / 0 B dl                                    ││+ dev-perl/IO-HTML                            │
│  sys-devel/mold (4 pkg, 2.9 MB)              ││[ ] perforce 0 B bin / 0 B dl                                ││(...)                                         │
│  dev-lisp/sbcl (4 pkg, 310.0 kB)             ││[x] perl -810.0 kB bin / -1.1 MB dl (+0/-12 pkg)             ││                                              │
│  app-admin/sudo (4 pkg, 2.4 MB)              ││[ ] python_single_target_python3_12 0 B bin / 0 B dl         ││cgi: Install gitweb too                       │
│  net-analyzer/ngrep (4 pkg, 850.0 kB)        ││[ ] python_single_target_python3_13 0 B bin / 0 B dl         ││                                              │
│  net-misc/chrony (4 pkg, 470.0 kB)           ││[x] python_single_target_python3_14 0 B bin / 0 B dl         ││                                              │
│  sys-fs/btrfs-progs (3 pkg, 2.3 MB)          ││[x] rust 0 B bin / 0 B dl                                    ││                                              │
│  net-misc/dhcp (3 pkg, 40.0 kB)              ││[x] safe-directory 0 B bin / 0 B dl                          ││                                              │
│  net-print/cups (3 pkg, 5.4 MB)              ││[ ] subversion +4.0 MB bin / +10.9 MB dl (+34/-0 pkg)        ││                                              │
│  app-portage/gentoolkit (3 pkg, 4.7 MB)      ││[ ] test 0 B bin / 0 B dl                                    ││                                              │
│  sys-apps/smartmontools (3 pkg, 2.7 MB)      ││[ ] tk +4.9 MB bin / +11.1 MB dl (+2/-0 pkg)                 ││                                              │
│  app-shells/bash-completion (3 pkg, 1.2 MB)  ││[x] webdav 0 B bin / 0 B dl                                  ││                                              │
│  net-wireless/iwd (3 pkg, 290.0 kB)          ││[ ] xinetd 0 B bin / 0 B dl                                  ││                                              │
│  app-misc/tmate (3 pkg, 300.0 kB)            ││                                                             ││                                              │
└──────────────────────────────────────────────┘└─────────────────────────────────────────────────────────────┘└──────────────────────────────────────────────┘
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 745.0 MB (486/537 Hits)  Quell-DL 3293.2 MB  Build-Only 243.2 MB bin / 1761.3 MB dl  Pakete 537  Konflikte 0
```
schalte da sachen aus die ich wohl nicht brauche
```
┌Pakete (553)──────────────────────────────────┐┌Details / USE-Flags──────────────────────────────────────────┐┌Blame / Pros-Cons─────────────────────────────┐
│  app-portage/eix (3 pkg, 820.0 kB)           ││dev-vcs/git  Binpkg: 9.8 MB  Pkg-Quell-DL: 8.7 MB            ││Warum ist das hier?                           │
│  net-dns/dnsmasq (3 pkg, 570.0 kB)           ││                                                             ││Kette 1:                                      │
│  net-im/signal-desktop-bin (2 pkg, 11.6 MB)  ││[ ] cgi 0 B bin / 0 B dl CONFLICT                            ││app-containers/docker -> dev-vcs/git          │
│  net-vpn/tailscale (2 pkg, 400.0 kB)         ││[x] curl 0 B bin / 0 B dl                                    ││                                              │
│  net-misc/freerdp (2 pkg, 20.0 MB)           ││[ ] cvs 0 B bin / 0 B dl CONFLICT                            ││Geteilte Abhaengigkeiten:                     │
│  sys-fs/cryptsetup (2 pkg, 1.4 MB)           ││[ ] debug 0 B bin / 0 B dl                                   ││app-alternatives/gpg                          │
│  net-misc/tigervnc (2 pkg, 6.1 MB)           ││[ ] doc 0 B bin / +1.7 MB dl                                 ││gehalten von app-crypt/gnupg                  │
│  app-misc/mc (2 pkg, 2.5 MB)                 ││[x] gpg 0 B bin / 0 B dl                                     ││gehalten von app-crypt/gpgme                  │
│  app-misc/fastfetch (2 pkg, 170.0 kB)        ││[ ] highlight 0 B bin / 0 B dl                               ││gehalten von app-portage/gemato               │
│  sys-fs/lvm2 (2 pkg, 500.0 kB)               ││[x] iconv 0 B bin / 0 B dl                                   ││gehalten von app-portage/getuto               │
│  app-misc/jq (2 pkg, 270.0 kB)               ││[ ] keyring +1.7 MB bin / +2.0 MB dl (+6/-0 pkg)             ││                                              │
│  media-video/pipewire (2 pkg, 2.6 MB)        ││[ ] nls 0 B bin / 0 B dl                                     ││gpg: Pull in gnupg for signing -- without     │
│  sys-apps/nvme-cli (2 pkg, 1.2 MB)           ││[x] pcre 0 B bin / 0 B dl                                    ││gnupg, attempts at signing will fail at       │
│  sys-kernel/dracut (2 pkg, 180.0 kB)         ││[ ] perforce 0 B bin / 0 B dl                                ││runtime!                                      │
│  sys-fs/erofs-utils (2 pkg, 350.0 kB)        ││[ ] perl +810.0 kB bin / +1.1 MB dl (+12/-0 pkg)             ││                                              │
│  sys-power/tlp (2 pkg, 120.0 kB)             ││[ ] python_single_target_python3_12 0 B bin / 0 B dl         ││                                              │
│  dev-build/ninja (2 pkg, 190.0 kB)           ││[ ] python_single_target_python3_13 0 B bin / 0 B dl         ││                                              │
│  sys-kernel/linux-firmware (1 pkg, 0 B)      ││[ ] python_single_target_python3_14 0 B bin / 0 B dl         ││                                              │
│  www-client/firefox-bin (1 pkg, 0 B)         ││[x] rust 0 B bin / 0 B dl                                    ││                                              │
│  dev-libs/nss (1 pkg, 2.5 MB)                ││[x] safe-directory 0 B bin / 0 B dl                          ││                                              │
│  x11-terms/alacritty (1 pkg, 0 B)            ││[ ] subversion 0 B bin / 0 B dl CONFLICT                     ││                                              │
│  dev-python/uv (1 pkg, 12.0 MB)              ││[ ] test 0 B bin / 0 B dl                                    ││                                              │
│  dev-debug/gdb (1 pkg, 4.6 MB)               ││[ ] tk +4.9 MB bin / +11.1 MB dl (+2/-0 pkg)                 ││                                              │
│  app-containers/docker-buildx (1 pkg, 16.4 MB││[ ] webdav 0 B bin / 0 B dl                                  ││                                              │
│  sys-firmware/sof-firmware (1 pkg, 0 B)      ││[ ] xinetd 0 B bin / 0 B dl                                  ││                                              │
│  sys-apps/ripgrep (1 pkg, 1.4 MB)            ││                                                             ││                                              │
│> dev-vcs/git (1 pkg, 9.8 MB)                 ││                                                             ││                                              │
└──────────────────────────────────────────────┘└─────────────────────────────────────────────────────────────┘└──────────────────────────────────────────────┘
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 744.2 MB (474/525 Hits)  Quell-DL 3292.1 MB  Build-Only 243.4 MB bin / 1761.9 MB dl  Pakete 525  Konflikte 0
```
schaue die gpg alternativen in der rechten column an:
```
┌Pakete (553)────────────────────────────┐┌Details / USE-Flags───────────────────────────────────┐┌Blame / Pros-Cons───────────────────────┐
│  net-im/signal-desktop-bin (2 pkg, 11.6││dev-vcs/git  Binpkg: 9.8 MB  Pkg-Quell-DL: 8.7 MB     ││Warum ist das hier?                     │
│  net-vpn/tailscale (2 pkg, 400.0 kB)   ││                                                      ││Kette 1:                                │
│  net-misc/freerdp (2 pkg, 20.0 MB)     ││[ ] cgi 0 B bin / 0 B dl CONFLICT                     ││app-containers/docker -> dev-vcs/git    │
│  sys-fs/cryptsetup (2 pkg, 1.4 MB)     ││[x] curl 0 B bin / 0 B dl                             ││                                        │
│  net-misc/tigervnc (2 pkg, 6.1 MB)     ││[ ] cvs 0 B bin / 0 B dl CONFLICT                     ││Geteilte Abhaengigkeiten:               │
│  app-misc/mc (2 pkg, 2.5 MB)           ││[ ] debug 0 B bin / 0 B dl                            ││> app-alternatives/gpg                  │
│  app-misc/fastfetch (2 pkg, 170.0 kB)  ││[ ] doc 0 B bin / +1.7 MB dl                          ││gehalten von app-crypt/gnupg            │
│  sys-fs/lvm2 (2 pkg, 500.0 kB)         ││[x] gpg 0 B bin / 0 B dl                              ││gehalten von app-crypt/gpgme            │
│  app-misc/jq (2 pkg, 270.0 kB)         ││[ ] highlight 0 B bin / 0 B dl                        ││gehalten von app-portage/gemato         │
│  media-video/pipewire (2 pkg, 2.6 MB)  ││[x] iconv 0 B bin / 0 B dl                            ││gehalten von app-portage/getuto         │
│  sys-apps/nvme-cli (2 pkg, 1.2 MB)     ││[ ] keyring +1.7 MB bin / +2.0 MB dl (+6/-0 pkg)      ││                                        │
│  sys-kernel/dracut (2 pkg, 180.0 kB)   ││[ ] nls 0 B bin / 0 B dl                              │└────────────────────────────────────────┘
│  sys-fs/erofs-utils (2 pkg, 350.0 kB)  ││[x] pcre 0 B bin / 0 B dl                             │┌Cut-Analyse─────────────────────────────┐
│  sys-power/tlp (2 pkg, 120.0 kB)       ││[ ] perforce 0 B bin / 0 B dl                         ││Cut-Analyse fuer app-alternatives/gpg:  │
│  dev-build/ninja (2 pkg, 190.0 kB)     ││[ ] perl +810.0 kB bin / +1.1 MB dl (+12/-0 pkg)      ││Flag-Blocker (Taste [a] wendet alle an):│
│  sys-kernel/linux-firmware (1 pkg, 0 B)││[ ] python_single_target_python3_12 0 B bin / 0 B dl  ││app-crypt/gnupg [-alternatives]         │
│  www-client/firefox-bin (1 pkg, 0 B)   ││[ ] python_single_target_python3_13 0 B bin / 0 B dl  ││app-portage/gemato [-gpg]               │
│  dev-libs/nss (1 pkg, 2.5 MB)          ││[ ] python_single_target_python3_14 0 B bin / 0 B dl  ││app-portage/portage-utils [+static]     │
│  x11-terms/alacritty (1 pkg, 0 B)      ││[x] rust 0 B bin / 0 B dl                             ││dev-vcs/git [-gpg]                      │
│  dev-python/uv (1 pkg, 12.0 MB)        ││[x] safe-directory 0 B bin / 0 B dl                   ││sys-apps/portage [+build]               │
│  dev-debug/gdb (1 pkg, 4.6 MB)         ││[ ] subversion 0 B bin / 0 B dl CONFLICT              ││sys-apps/portage [-rsync-verify]        │
│  app-containers/docker-buildx (1 pkg, 1││[ ] test 0 B bin / 0 B dl                             ││sys-apps/smartmontools [-update-drivedb]│
│  sys-firmware/sof-firmware (1 pkg, 0 B)││[ ] tk +4.9 MB bin / +11.1 MB dl (+2/-0 pkg)          ││keine harten world-Blocker              │
│  sys-apps/ripgrep (1 pkg, 1.4 MB)      ││[ ] webdav 0 B bin / 0 B dl                           ││Ohne Flag-Blocker: -5.7 MB bin / -14.9  │
│> dev-vcs/git (1 pkg, 9.8 MB)           ││[ ] xinetd 0 B bin / 0 B dl                           ││MB dl (-18/+0 pkg)                      │
└────────────────────────────────────────┘└──────────────────────────────────────────────────────┘└────────────────────────────────────────┘
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 744.2 MB (474/525 Hits)  Quell-DL 3292.1 MB  Build-Only 243.4 MB bin / 1761.9 MB dl  Pakete 525  Konflikte 0
```
```
┌Pakete (553)────────────────────────────┐┌Details / USE-Flags───────────────────────────────────┐┌Blame / Pros-Cons───────────────────────┐
│  net-im/signal-desktop-bin (2 pkg, 11.6││dev-vcs/git  Binpkg: 9.8 MB  Pkg-Quell-DL: 8.7 MB     ││Warum ist das hier?                     │
│  net-vpn/tailscale (2 pkg, 400.0 kB)   ││                                                      ││Kette 1:                                │
│  net-misc/freerdp (2 pkg, 20.0 MB)     ││[ ] cgi 0 B bin / 0 B dl CONFLICT                     ││app-containers/docker -> dev-vcs/git    │
│  sys-fs/cryptsetup (2 pkg, 1.4 MB)     ││[x] curl 0 B bin / 0 B dl                             ││                                        │
│  net-misc/tigervnc (2 pkg, 6.1 MB)     ││[ ] cvs 0 B bin / 0 B dl CONFLICT                     ││Geteilte Abhaengigkeiten:               │
│  app-misc/mc (2 pkg, 2.5 MB)           ││[ ] debug 0 B bin / 0 B dl                            ││app-alternatives/gpg                    │
│  app-misc/fastfetch (2 pkg, 170.0 kB)  ││[ ] doc 0 B bin / +1.7 MB dl                          ││>   gehalten von app-crypt/gnupg        │
│  sys-fs/lvm2 (2 pkg, 500.0 kB)         ││[x] gpg 0 B bin / 0 B dl                              ││gehalten von app-crypt/gpgme            │
│  app-misc/jq (2 pkg, 270.0 kB)         ││[ ] highlight 0 B bin / 0 B dl                        ││gehalten von app-portage/gemato         │
│  media-video/pipewire (2 pkg, 2.6 MB)  ││[x] iconv 0 B bin / 0 B dl                            ││gehalten von app-portage/getuto         │
│  sys-apps/nvme-cli (2 pkg, 1.2 MB)     ││[ ] keyring +1.7 MB bin / +2.0 MB dl (+6/-0 pkg)      ││                                        │
│  sys-kernel/dracut (2 pkg, 180.0 kB)   ││[ ] nls 0 B bin / 0 B dl                              │└────────────────────────────────────────┘
│  sys-fs/erofs-utils (2 pkg, 350.0 kB)  ││[x] pcre 0 B bin / 0 B dl                             │┌Cut-Analyse─────────────────────────────┐
│  sys-power/tlp (2 pkg, 120.0 kB)       ││[ ] perforce 0 B bin / 0 B dl                         ││Cut-Analyse fuer app-crypt/gnupg:       │
│  dev-build/ninja (2 pkg, 190.0 kB)     ││[ ] perl +810.0 kB bin / +1.1 MB dl (+12/-0 pkg)      ││Flag-Blocker (Taste [a] wendet alle an):│
│  sys-kernel/linux-firmware (1 pkg, 0 B)││[ ] python_single_target_python3_12 0 B bin / 0 B dl  ││app-crypt/gnupg [-alternatives]         │
│  www-client/firefox-bin (1 pkg, 0 B)   ││[ ] python_single_target_python3_13 0 B bin / 0 B dl  ││app-portage/gemato [-gpg]               │
│  dev-libs/nss (1 pkg, 2.5 MB)          ││[ ] python_single_target_python3_14 0 B bin / 0 B dl  ││app-portage/portage-utils [+static]     │
│  x11-terms/alacritty (1 pkg, 0 B)      ││[x] rust 0 B bin / 0 B dl                             ││dev-vcs/git [-gpg]                      │
│  dev-python/uv (1 pkg, 12.0 MB)        ││[x] safe-directory 0 B bin / 0 B dl                   ││sys-apps/portage [+build]               │
│  dev-debug/gdb (1 pkg, 4.6 MB)         ││[ ] subversion 0 B bin / 0 B dl CONFLICT              ││sys-apps/portage [-rsync-verify]        │
│  app-containers/docker-buildx (1 pkg, 1││[ ] test 0 B bin / 0 B dl                             ││sys-apps/smartmontools [-update-drivedb]│
│  sys-firmware/sof-firmware (1 pkg, 0 B)││[ ] tk +4.9 MB bin / +11.1 MB dl (+2/-0 pkg)          ││keine harten world-Blocker              │
│  sys-apps/ripgrep (1 pkg, 1.4 MB)      ││[ ] webdav 0 B bin / 0 B dl                           ││Ohne Flag-Blocker: -5.7 MB bin / -14.9  │
│> dev-vcs/git (1 pkg, 9.8 MB)           ││[ ] xinetd 0 B bin / 0 B dl                           ││MB dl (-18/+0 pkg)                      │
└────────────────────────────────────────┘└──────────────────────────────────────────────────────┘└────────────────────────────────────────┘
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 744.2 MB (474/525 Hits)  Quell-DL 3292.1 MB  Build-Only 243.4 MB bin / 1761.9 MB dl  Pakete 525  Konflikte 0
```

```
┌Pakete (553)────────────────────────────┐┌Details / USE-Flags───────────────────────────────────┐┌Blame / Pros-Cons───────────────────────┐
│  net-im/signal-desktop-bin (2 pkg, 11.6││dev-vcs/git  Binpkg: 9.8 MB  Pkg-Quell-DL: 8.7 MB     ││Warum ist das hier?                     │
│  net-vpn/tailscale (2 pkg, 400.0 kB)   ││                                                      ││Kette 1:                                │
│  net-misc/freerdp (2 pkg, 20.0 MB)     ││[ ] cgi 0 B bin / 0 B dl CONFLICT                     ││app-containers/docker -> dev-vcs/git    │
│  sys-fs/cryptsetup (2 pkg, 1.4 MB)     ││[x] curl 0 B bin / 0 B dl                             ││                                        │
│  net-misc/tigervnc (2 pkg, 6.1 MB)     ││[ ] cvs 0 B bin / 0 B dl CONFLICT                     ││Geteilte Abhaengigkeiten:               │
│  app-misc/mc (2 pkg, 2.5 MB)           ││[ ] debug 0 B bin / 0 B dl                            ││app-alternatives/gpg                    │
│  app-misc/fastfetch (2 pkg, 170.0 kB)  ││[ ] doc 0 B bin / +1.7 MB dl                          ││gehalten von app-crypt/gnupg            │
│  sys-fs/lvm2 (2 pkg, 500.0 kB)         ││[x] gpg 0 B bin / 0 B dl                              ││>   gehalten von app-crypt/gpgme        │
│  app-misc/jq (2 pkg, 270.0 kB)         ││[ ] highlight 0 B bin / 0 B dl                        ││gehalten von app-portage/gemato         │
│  media-video/pipewire (2 pkg, 2.6 MB)  ││[x] iconv 0 B bin / 0 B dl                            ││gehalten von app-portage/getuto         │
│  sys-apps/nvme-cli (2 pkg, 1.2 MB)     ││[ ] keyring +1.7 MB bin / +2.0 MB dl (+6/-0 pkg)      ││                                        │
│  sys-kernel/dracut (2 pkg, 180.0 kB)   ││[ ] nls 0 B bin / 0 B dl                              ││gpg: Pull in gnupg for signing --       │
│  sys-fs/erofs-utils (2 pkg, 350.0 kB)  ││[x] pcre 0 B bin / 0 B dl                             ││without gnupg, attempts at signing will │
│  sys-power/tlp (2 pkg, 120.0 kB)       ││[ ] perforce 0 B bin / 0 B dl                         ││fail at runtime!                        │
│  dev-build/ninja (2 pkg, 190.0 kB)     ││[ ] perl +810.0 kB bin / +1.1 MB dl (+12/-0 pkg)      ││                                        │
│  sys-kernel/linux-firmware (1 pkg, 0 B)││[ ] python_single_target_python3_12 0 B bin / 0 B dl  ││                                        │
│  www-client/firefox-bin (1 pkg, 0 B)   ││[ ] python_single_target_python3_13 0 B bin / 0 B dl  ││                                        │
│  dev-libs/nss (1 pkg, 2.5 MB)          ││[ ] python_single_target_python3_14 0 B bin / 0 B dl  │└────────────────────────────────────────┘
│  x11-terms/alacritty (1 pkg, 0 B)      ││[x] rust 0 B bin / 0 B dl                             │┌Cut-Analyse─────────────────────────────┐
│  dev-python/uv (1 pkg, 12.0 MB)        ││[x] safe-directory 0 B bin / 0 B dl                   ││Cut-Analyse fuer app-crypt/gpgme:       │
│  dev-debug/gdb (1 pkg, 4.6 MB)         ││[ ] subversion 0 B bin / 0 B dl CONFLICT              ││Flag-Blocker (Taste [a] wendet alle an):│
│  app-containers/docker-buildx (1 pkg, 1││[ ] test 0 B bin / 0 B dl                             ││app-portage/portage-utils [+static]     │
│  sys-firmware/sof-firmware (1 pkg, 0 B)││[ ] tk +4.9 MB bin / +11.1 MB dl (+2/-0 pkg)          ││keine harten world-Blocker              │
│  sys-apps/ripgrep (1 pkg, 1.4 MB)      ││[ ] webdav 0 B bin / 0 B dl                           ││Ohne Flag-Blocker: -500.0 kB bin / -1.4 │
│> dev-vcs/git (1 pkg, 9.8 MB)           ││[ ] xinetd 0 B bin / 0 B dl                           ││MB dl (-1/+0 pkg)                       │
└────────────────────────────────────────┘└──────────────────────────────────────────────────────┘└────────────────────────────────────────┘
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 744.2 MB (474/525 Hits)  Quell-DL 3292.1 MB  Build-Only 243.4 MB bin / 1761.9 MB dl  Pakete 525  Konflikte 0
```

```
Pakete (553)────────────────────────────┐┌Details / USE-Flags───────────────────────────────────┐┌Blame / Pros-Cons───────────────────────┐
│  net-im/signal-desktop-bin (2 pkg, 11.6││dev-vcs/git  Binpkg: 9.8 MB  Pkg-Quell-DL: 8.7 MB     ││Warum ist das hier?                     │
│  net-vpn/tailscale (2 pkg, 400.0 kB)   ││                                                      ││Kette 1:                                │
│  net-misc/freerdp (2 pkg, 20.0 MB)     ││[ ] cgi 0 B bin / 0 B dl CONFLICT                     ││app-containers/docker -> dev-vcs/git    │
│  sys-fs/cryptsetup (2 pkg, 1.4 MB)     ││[x] curl 0 B bin / 0 B dl                             ││                                        │
│  net-misc/tigervnc (2 pkg, 6.1 MB)     ││[ ] cvs 0 B bin / 0 B dl CONFLICT                     ││Geteilte Abhaengigkeiten:               │
│  app-misc/mc (2 pkg, 2.5 MB)           ││[ ] debug 0 B bin / 0 B dl                            ││app-alternatives/gpg                    │
│  app-misc/fastfetch (2 pkg, 170.0 kB)  ││[ ] doc 0 B bin / +1.7 MB dl                          ││gehalten von app-crypt/gnupg            │
│  sys-fs/lvm2 (2 pkg, 500.0 kB)         ││[x] gpg 0 B bin / 0 B dl                              ││gehalten von app-crypt/gpgme            │
│  app-misc/jq (2 pkg, 270.0 kB)         ││[ ] highlight 0 B bin / 0 B dl                        ││>   gehalten von app-portage/gemato     │
│  media-video/pipewire (2 pkg, 2.6 MB)  ││[x] iconv 0 B bin / 0 B dl                            ││gehalten von app-portage/getuto         │
│  sys-apps/nvme-cli (2 pkg, 1.2 MB)     ││[ ] keyring +1.7 MB bin / +2.0 MB dl (+6/-0 pkg)      ││                                        │
│  sys-kernel/dracut (2 pkg, 180.0 kB)   ││[ ] nls 0 B bin / 0 B dl                              ││gpg: Pull in gnupg for signing --       │
│  sys-fs/erofs-utils (2 pkg, 350.0 kB)  ││[x] pcre 0 B bin / 0 B dl                             ││without gnupg, attempts at signing will │
│  sys-power/tlp (2 pkg, 120.0 kB)       ││[ ] perforce 0 B bin / 0 B dl                         ││fail at runtime!                        │
│  dev-build/ninja (2 pkg, 190.0 kB)     ││[ ] perl +810.0 kB bin / +1.1 MB dl (+12/-0 pkg)      ││                                        │
│  sys-kernel/linux-firmware (1 pkg, 0 B)││[ ] python_single_target_python3_12 0 B bin / 0 B dl  ││                                        │
│  www-client/firefox-bin (1 pkg, 0 B)   ││[ ] python_single_target_python3_13 0 B bin / 0 B dl  ││                                        │
│  dev-libs/nss (1 pkg, 2.5 MB)          ││[ ] python_single_target_python3_14 0 B bin / 0 B dl  │└────────────────────────────────────────┘
│  x11-terms/alacritty (1 pkg, 0 B)      ││[x] rust 0 B bin / 0 B dl                             │┌Cut-Analyse─────────────────────────────┐
│  dev-python/uv (1 pkg, 12.0 MB)        ││[x] safe-directory 0 B bin / 0 B dl                   ││Cut-Analyse fuer app-portage/gemato:    │
│  dev-debug/gdb (1 pkg, 4.6 MB)         ││[ ] subversion 0 B bin / 0 B dl CONFLICT              ││Flag-Blocker (Taste [a] wendet alle an):│
│  app-containers/docker-buildx (1 pkg, 1││[ ] test 0 B bin / 0 B dl                             ││sys-apps/portage [-rsync-verify]        │
│  sys-firmware/sof-firmware (1 pkg, 0 B)││[ ] tk +4.9 MB bin / +11.1 MB dl (+2/-0 pkg)          ││keine harten world-Blocker              │
│  sys-apps/ripgrep (1 pkg, 1.4 MB)      ││[ ] webdav 0 B bin / 0 B dl                           ││Ohne Flag-Blocker: -1.0 MB bin / -1.3 MB│
│> dev-vcs/git (1 pkg, 9.8 MB)           ││[ ] xinetd 0 B bin / 0 B dl                           ││dl (-7/+0 pkg)                          │
└────────────────────────────────────────┘└──────────────────────────────────────────────────────┘└────────────────────────────────────────┘
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 744.2 MB (474/525 Hits)  Quell-DL 3292.1 MB  Build-Only 243.4 MB bin / 1761.9 MB dl  Pakete 525  Konflikte 0
```
```
┌Pakete (553)────────────────────────────┐┌Details / USE-Flags───────────────────────────────────┐┌Blame / Pros-Cons───────────────────────┐
│  net-im/signal-desktop-bin (2 pkg, 11.6││dev-vcs/git  Binpkg: 9.8 MB  Pkg-Quell-DL: 8.7 MB     ││Warum ist das hier?                     │
│  net-vpn/tailscale (2 pkg, 400.0 kB)   ││                                                      ││Kette 1:                                │
│  net-misc/freerdp (2 pkg, 20.0 MB)     ││[ ] cgi 0 B bin / 0 B dl CONFLICT                     ││app-containers/docker -> dev-vcs/git    │
│  sys-fs/cryptsetup (2 pkg, 1.4 MB)     ││[x] curl 0 B bin / 0 B dl                             ││                                        │
│  net-misc/tigervnc (2 pkg, 6.1 MB)     ││[ ] cvs 0 B bin / 0 B dl CONFLICT                     ││Geteilte Abhaengigkeiten:               │
│  app-misc/mc (2 pkg, 2.5 MB)           ││[ ] debug 0 B bin / 0 B dl                            ││app-alternatives/gpg                    │
│  app-misc/fastfetch (2 pkg, 170.0 kB)  ││[ ] doc 0 B bin / +1.7 MB dl                          ││gehalten von app-crypt/gnupg            │
│  sys-fs/lvm2 (2 pkg, 500.0 kB)         ││[x] gpg 0 B bin / 0 B dl                              ││gehalten von app-crypt/gpgme            │
│  app-misc/jq (2 pkg, 270.0 kB)         ││[ ] highlight 0 B bin / 0 B dl                        ││gehalten von app-portage/gemato         │
│  media-video/pipewire (2 pkg, 2.6 MB)  ││[x] iconv 0 B bin / 0 B dl                            ││>   gehalten von app-portage/getuto     │
│  sys-apps/nvme-cli (2 pkg, 1.2 MB)     ││[ ] keyring +1.7 MB bin / +2.0 MB dl (+6/-0 pkg)      ││                                        │
│  sys-kernel/dracut (2 pkg, 180.0 kB)   ││[ ] nls 0 B bin / 0 B dl                              ││gpg: Pull in gnupg for signing --       │
│  sys-fs/erofs-utils (2 pkg, 350.0 kB)  ││[x] pcre 0 B bin / 0 B dl                             ││without gnupg, attempts at signing will │
│  sys-power/tlp (2 pkg, 120.0 kB)       ││[ ] perforce 0 B bin / 0 B dl                         ││fail at runtime!                        │
│  dev-build/ninja (2 pkg, 190.0 kB)     ││[ ] perl +810.0 kB bin / +1.1 MB dl (+12/-0 pkg)      ││                                        │
│  sys-kernel/linux-firmware (1 pkg, 0 B)││[ ] python_single_target_python3_12 0 B bin / 0 B dl  ││                                        │
│  www-client/firefox-bin (1 pkg, 0 B)   ││[ ] python_single_target_python3_13 0 B bin / 0 B dl  ││                                        │
│  dev-libs/nss (1 pkg, 2.5 MB)          ││[ ] python_single_target_python3_14 0 B bin / 0 B dl  │└────────────────────────────────────────┘
│  x11-terms/alacritty (1 pkg, 0 B)      ││[x] rust 0 B bin / 0 B dl                             │┌Cut-Analyse─────────────────────────────┐
│  dev-python/uv (1 pkg, 12.0 MB)        ││[x] safe-directory 0 B bin / 0 B dl                   ││Cut-Analyse fuer app-portage/getuto:    │
│  dev-debug/gdb (1 pkg, 4.6 MB)         ││[ ] subversion 0 B bin / 0 B dl CONFLICT              ││Flag-Blocker (Taste [a] wendet alle an):│
│  app-containers/docker-buildx (1 pkg, 1││[ ] test 0 B bin / 0 B dl                             ││sys-apps/portage [+build]               │
│  sys-firmware/sof-firmware (1 pkg, 0 B)││[ ] tk +4.9 MB bin / +11.1 MB dl (+2/-0 pkg)          ││keine harten world-Blocker              │
│  sys-apps/ripgrep (1 pkg, 1.4 MB)      ││[ ] webdav 0 B bin / 0 B dl                           ││Ohne Flag-Blocker: -1.6 MB bin / -3.4 MB│
│> dev-vcs/git (1 pkg, 9.8 MB)           ││[ ] xinetd 0 B bin / 0 B dl                           ││dl (-11/+0 pkg)                         │
└────────────────────────────────────────┘└──────────────────────────────────────────────────────┘└────────────────────────────────────────┘
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 744.2 MB (474/525 Hits)  Quell-DL 3292.1 MB  Build-Only 243.4 MB bin / 1761.9 MB dl  Pakete 525  Konflikte 0
```

druecke a auf gpgme

```
┌Pakete (551)────────────────────────────┐┌Details / USE-Flags───────────────────────────────────┐┌Blame / Pros-Cons───────────────────────┐
│  net-im/signal-desktop-bin (2 pkg, 11.6││dev-vcs/git  Binpkg: 9.8 MB  Pkg-Quell-DL: 8.7 MB     ││Warum ist das hier?                     │
│  net-vpn/tailscale (2 pkg, 400.0 kB)   ││                                                      ││Kette 1:                                │
│  net-misc/freerdp (2 pkg, 20.0 MB)     ││[ ] cgi 0 B bin / 0 B dl CONFLICT                     ││app-containers/docker -> dev-vcs/git    │
│  sys-fs/cryptsetup (2 pkg, 1.4 MB)     ││[x] curl 0 B bin / 0 B dl                             ││                                        │
│  net-misc/tigervnc (2 pkg, 6.1 MB)     ││[ ] cvs 0 B bin / 0 B dl CONFLICT                     ││Geteilte Abhaengigkeiten:               │
│  app-misc/mc (2 pkg, 2.5 MB)           ││[ ] debug 0 B bin / 0 B dl                            ││app-alternatives/gpg                    │
│  app-misc/fastfetch (2 pkg, 170.0 kB)  ││[ ] doc 0 B bin / +1.7 MB dl                          ││gehalten von app-crypt/gnupg            │
│  sys-fs/lvm2 (2 pkg, 500.0 kB)         ││[x] gpg 0 B bin / 0 B dl                              ││gehalten von app-portage/gemato         │
│  app-misc/jq (2 pkg, 270.0 kB)         ││[ ] highlight 0 B bin / 0 B dl                        ││gehalten von app-portage/getuto         │
│  media-video/pipewire (2 pkg, 2.6 MB)  ││[x] iconv 0 B bin / 0 B dl                            ││gehalten von sys-apps/portage           │
│  sys-apps/nvme-cli (2 pkg, 1.2 MB)     ││[ ] keyring +1.7 MB bin / +2.0 MB dl (+6/-0 pkg)      ││                                        │
│  sys-kernel/dracut (2 pkg, 180.0 kB)   ││[ ] nls 0 B bin / 0 B dl                              ││gpg: Pull in gnupg for signing --       │
│  sys-fs/erofs-utils (2 pkg, 350.0 kB)  ││[x] pcre 0 B bin / 0 B dl                             ││without gnupg, attempts at signing will │
│  sys-power/tlp (2 pkg, 120.0 kB)       ││[ ] perforce 0 B bin / 0 B dl                         ││fail at runtime!                        │
│  dev-build/ninja (2 pkg, 190.0 kB)     ││[ ] perl +810.0 kB bin / +1.1 MB dl (+12/-0 pkg)      ││                                        │
│  sys-kernel/linux-firmware (1 pkg, 0 B)││[ ] python_single_target_python3_12 0 B bin / 0 B dl  ││                                        │
│  www-client/firefox-bin (1 pkg, 0 B)   ││[ ] python_single_target_python3_13 0 B bin / 0 B dl  ││                                        │
│  dev-libs/nss (1 pkg, 2.5 MB)          ││[ ] python_single_target_python3_14 0 B bin / 0 B dl  ││                                        │
│  x11-terms/alacritty (1 pkg, 0 B)      ││[x] rust 0 B bin / 0 B dl                             ││                                        │
│  dev-python/uv (1 pkg, 12.0 MB)        ││[x] safe-directory 0 B bin / 0 B dl                   ││                                        │
│  dev-debug/gdb (1 pkg, 4.6 MB)         ││[ ] subversion 0 B bin / 0 B dl CONFLICT              ││                                        │
│  app-containers/docker-buildx (1 pkg, 1││[ ] test 0 B bin / 0 B dl                             ││                                        │
│  sys-firmware/sof-firmware (1 pkg, 0 B)││[ ] tk +4.9 MB bin / +11.1 MB dl (+2/-0 pkg)          ││                                        │
│  sys-apps/ripgrep (1 pkg, 1.4 MB)      ││[ ] webdav 0 B bin / 0 B dl                           ││                                        │
│> dev-vcs/git (1 pkg, 9.8 MB)           ││[ ] xinetd 0 B bin / 0 B dl                           ││                                        │
└────────────────────────────────────────┘└──────────────────────────────────────────────────────┘└────────────────────────────────────────┘
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
Binpkg 743.7 MB (473/524 Hits)  Quell-DL 3290.8 MB  Build-Only 243.9 MB bin / 1763.3 MB dl  Pakete 524  Konflikte 0
[Space] Toggle  [Tab] Pane  [1/2] View  [/] Suche  [o] Sort  [x] Cut  [a] Apply  [u] Undo  [S] package.use  [?] Hilfe  [q] Quit
*ungespeichert*  Cut-Blocker angewendet
```

```
kiel@localhost ~ $ cat /tmp/gentoo-dep-tui.package.use 
app-portage/portage-utils static
dev-vcs/git -nls -perl -python_single_target_python3_14 -webdav
sys-power/tlp -ppd -python_single_target_python3_14 -rdw
```
