#!/usr/bin/env bash
# Builds release artifacts: portable tarball and .deb package.
set -euo pipefail
cd /opt/zapfast
V=0.19.1
ARCH=amd64

# Portable tarball with the update marker.
rm -rf "/tmp/zapfast-portable-$V"
mkdir -p "/tmp/zapfast-portable-$V"
cp target/release/zapfast "/tmp/zapfast-portable-$V/"
cp packaging/zapfast-portable.txt README.md LICENSE "/tmp/zapfast-portable-$V/"
rm -f "zapfast-v$V-x86_64-linux.tar.gz"
tar -c -z -f "zapfast-v$V-x86_64-linux.tar.gz" -C /tmp "zapfast-portable-$V"

# .deb package layout.
DEB="/tmp/zapfast-deb-$V"
rm -rf "$DEB"
mkdir -p "$DEB/DEBIAN" \
  "$DEB/usr/bin" \
  "$DEB/usr/share/applications" \
  "$DEB/usr/share/icons/hicolor/scalable/apps" \
  "$DEB/usr/share/doc/zapfast" \
  "$DEB/usr/share/metainfo"
install -m 755 target/release/zapfast "$DEB/usr/bin/zapfast"
install -m 644 packaging/applications/zapfast.desktop "$DEB/usr/share/applications/"
install -m 644 packaging/icons/zapfast.svg "$DEB/usr/share/icons/hicolor/scalable/apps/"
install -m 644 README.md "$DEB/usr/share/doc/zapfast/"
install -m 644 LICENSE "$DEB/usr/share/doc/zapfast/copyright"

cat > "$DEB/DEBIAN/control" <<EOF
Package: zapfast
Version: $V
Section: net
Priority: optional
Architecture: $ARCH
Depends: libasound2t64 (>= 1.2) | libasound2 (>= 1.2)
Maintainer: abhinavdatta <abhinavdatta@users.noreply.github.com>
Homepage: https://github.com/abhinavdatta/zapfast
Description: A fast, native WhatsApp client
 ZapFast links to your phone as a companion device, keeps an encrypted
 local archive, and stays in the system tray. Built with Rust and egui.
EOF

rm -f "zapfast-v$V-amd64.deb"
dpkg-deb --root-owner-group --build "$DEB" "zapfast-v$V-amd64.deb"
ls -la "zapfast-v$V-x86_64-linux.tar.gz" "zapfast-v$V-amd64.deb"
