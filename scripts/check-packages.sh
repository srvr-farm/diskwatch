#!/bin/sh
set -eu

: "${BIN:?BIN is required}"
: "${VERSION:?VERSION is required}"
: "${CAPABILITY:?CAPABILITY is required}"

PACKAGE_OUTPUT_DIR="${PACKAGE_OUTPUT_DIR:-dist}"
DEB_ARCH="${DEB_ARCH:-amd64}"
RPM_ARCH="${RPM_ARCH:-x86_64}"
RPM_RELEASE="${RPM_RELEASE:-1}"

deb="${PACKAGE_OUTPUT_DIR}/${BIN}_${VERSION}_${DEB_ARCH}.deb"
rpm="${PACKAGE_OUTPUT_DIR}/${BIN}-${VERSION}-${RPM_RELEASE}.${RPM_ARCH}.rpm"

for artifact in "$deb" "$deb.sha256" "$rpm" "$rpm.sha256"; do
  test -s "$artifact" || {
    echo "missing package artifact: $artifact" >&2
    exit 1
  }
done

sha256sum -c "$deb.sha256"
sha256sum -c "$rpm.sha256"

test "$(dpkg-deb -f "$deb" Package)" = "$BIN"
test "$(dpkg-deb -f "$deb" Version)" = "$VERSION"
test "$(dpkg-deb -f "$deb" Architecture)" = "$DEB_ARCH"
dpkg-deb -f "$deb" Depends | grep -F "libcap2-bin" >/dev/null
dpkg-deb -c "$deb" | grep -Eq "^-rwxr-xr-x +root/root +[0-9]+ .* \\./usr/bin/${BIN}$"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
dpkg-deb --control "$deb" "$tmp/deb-control"
grep -F "chmod 0755 /usr/bin/${BIN}" "$tmp/deb-control/postinst" >/dev/null
grep -F "setcap ${CAPABILITY} /usr/bin/${BIN}" "$tmp/deb-control/postinst" >/dev/null

test "$(rpm -qp --qf '%{NAME}' "$rpm")" = "$BIN"
test "$(rpm -qp --qf '%{VERSION}' "$rpm")" = "$VERSION"
test "$(rpm -qp --qf '%{RELEASE}' "$rpm")" = "$RPM_RELEASE"
test "$(rpm -qp --qf '%{ARCH}' "$rpm")" = "$RPM_ARCH"
rpm -qpR "$rpm" | grep -F "libcap" >/dev/null
rpm -qplv "$rpm" | grep -Eq "^-rwxr-xr-x +1 root +root +[0-9]+ .* /usr/bin/${BIN}$"
rpm -qp --scripts "$rpm" | grep -F "chmod 0755 /usr/bin/${BIN}" >/dev/null
rpm -qp --scripts "$rpm" | grep -F "setcap ${CAPABILITY} /usr/bin/${BIN}" >/dev/null

echo "package checks passed for ${BIN} ${VERSION}"
