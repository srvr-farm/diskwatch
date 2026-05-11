PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
BIN ?= diskwatch
INSTALL_PATH ?= $(BINDIR)/$(BIN)
CAPABILITY ?= cap_dac_read_search+ep
PACKAGE_OUTPUT_DIR ?= dist
VERSION ?= $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
DEB_ARCH ?= amd64
RPM_ARCH ?= x86_64
RPM_RELEASE ?= 1
PACKAGE_MAINTAINER ?= srvr-farm <noreply@srvr.farm>
PACKAGE_URL ?= https://github.com/srvr-farm/$(BIN)
PACKAGE_SUMMARY ?= Linux storage monitor terminal TUI
PACKAGE_DESCRIPTION ?= Read-only Linux storage monitor for disk activity, filesystem space, block devices, ZFS pools, RAID arrays, LVM, and SMART health.

CARGO ?= cargo
INSTALL ?= install
SETCAP ?= setcap
GETCAP ?= getcap

ifeq ($(shell id -u),0)
SUDO ?=
else
SUDO ?= sudo
endif

BUILD_BIN := target/release/$(BIN)
PACKAGE_BUILD_DIR := target/package/$(BIN)-$(VERSION)
DEB_STAGING := $(PACKAGE_BUILD_DIR)/deb
RPM_TOPDIR := $(CURDIR)/target/rpmbuild/$(BIN)-$(VERSION)
DEB_PACKAGE := $(PACKAGE_OUTPUT_DIR)/$(BIN)_$(VERSION)_$(DEB_ARCH).deb
RPM_PACKAGE := $(PACKAGE_OUTPUT_DIR)/$(BIN)-$(VERSION)-$(RPM_RELEASE).$(RPM_ARCH).rpm

.PHONY: all build install ensure-install-build install-binary capability show-capability uninstall test fmt clippy check package package-deb package-rpm check-packages package-clean clean

all: build

build:
	$(CARGO) build --release

install: install-binary
	$(SUDO) $(SETCAP) $(CAPABILITY) $(INSTALL_PATH)
	$(GETCAP) $(INSTALL_PATH)

ensure-install-build:
	@if [ "$$(id -u)" -eq 0 ]; then \
		test -x "$(BUILD_BIN)" || { \
			echo "$(BUILD_BIN) is missing; run 'make build' before 'sudo make install'."; \
			exit 1; \
		}; \
	else \
		$(MAKE) build; \
	fi

install-binary: ensure-install-build
	$(SUDO) $(INSTALL) -d $(dir $(INSTALL_PATH))
	$(SUDO) $(INSTALL) -m 0755 $(BUILD_BIN) $(INSTALL_PATH)

capability:
	$(SUDO) $(SETCAP) $(CAPABILITY) $(INSTALL_PATH)
	$(GETCAP) $(INSTALL_PATH)

show-capability:
	$(GETCAP) $(INSTALL_PATH)

uninstall:
	$(SUDO) rm -f $(INSTALL_PATH)

test:
	$(CARGO) test

fmt:
	$(CARGO) fmt --check

clippy:
	$(CARGO) clippy -- -D warnings

check: fmt test clippy

package: package-deb package-rpm

package-deb: build
	@command -v dpkg-deb >/dev/null 2>&1 || { echo "dpkg-deb is required to build Debian packages."; exit 1; }
	rm -rf $(DEB_STAGING)
	$(INSTALL) -d $(DEB_STAGING)/DEBIAN $(DEB_STAGING)/usr/bin $(PACKAGE_OUTPUT_DIR)
	$(INSTALL) -m 0755 $(BUILD_BIN) $(DEB_STAGING)/usr/bin/$(BIN)
	sed \
		-e 's|@BIN@|$(BIN)|g' \
		-e 's|@VERSION@|$(VERSION)|g' \
		-e 's|@DEB_ARCH@|$(DEB_ARCH)|g' \
		-e 's|@MAINTAINER@|$(PACKAGE_MAINTAINER)|g' \
		-e 's|@URL@|$(PACKAGE_URL)|g' \
		-e 's|@SUMMARY@|$(PACKAGE_SUMMARY)|g' \
		-e 's|@DESCRIPTION@|$(PACKAGE_DESCRIPTION)|g' \
		packaging/deb/control.template > $(DEB_STAGING)/DEBIAN/control
	sed \
		-e 's|@BIN@|$(BIN)|g' \
		-e 's|@CAPABILITY@|$(CAPABILITY)|g' \
		packaging/deb/postinst.template > $(DEB_STAGING)/DEBIAN/postinst
	chmod 0755 $(DEB_STAGING)/DEBIAN/postinst
	dpkg-deb --build --root-owner-group $(DEB_STAGING) $(DEB_PACKAGE)
	sha256sum $(DEB_PACKAGE) > $(DEB_PACKAGE).sha256

package-rpm: build
	@command -v rpmbuild >/dev/null 2>&1 || { echo "rpmbuild is required to build RPM packages."; exit 1; }
	rm -rf $(RPM_TOPDIR)
	$(INSTALL) -d $(RPM_TOPDIR)/BUILD $(RPM_TOPDIR)/BUILDROOT $(RPM_TOPDIR)/RPMS $(RPM_TOPDIR)/SOURCES $(RPM_TOPDIR)/SPECS $(RPM_TOPDIR)/SRPMS $(PACKAGE_OUTPUT_DIR)
	sed \
		-e 's|@BIN@|$(BIN)|g' \
		-e 's|@VERSION@|$(VERSION)|g' \
		-e 's|@RPM_RELEASE@|$(RPM_RELEASE)|g' \
		-e 's|@RPM_ARCH@|$(RPM_ARCH)|g' \
		-e 's|@URL@|$(PACKAGE_URL)|g' \
		-e 's|@SUMMARY@|$(PACKAGE_SUMMARY)|g' \
		-e 's|@DESCRIPTION@|$(PACKAGE_DESCRIPTION)|g' \
		-e 's|@BUILD_BIN@|$(CURDIR)/$(BUILD_BIN)|g' \
		-e 's|@CAPABILITY@|$(CAPABILITY)|g' \
		packaging/rpm/package.spec.template > $(RPM_TOPDIR)/SPECS/$(BIN).spec
	rpmbuild --define '_topdir $(RPM_TOPDIR)' --define '_build_id_links none' -bb $(RPM_TOPDIR)/SPECS/$(BIN).spec
	cp $(RPM_TOPDIR)/RPMS/$(RPM_ARCH)/$(BIN)-$(VERSION)-$(RPM_RELEASE).$(RPM_ARCH).rpm $(RPM_PACKAGE)
	sha256sum $(RPM_PACKAGE) > $(RPM_PACKAGE).sha256

check-packages:
	BIN=$(BIN) VERSION=$(VERSION) CAPABILITY=$(CAPABILITY) PACKAGE_OUTPUT_DIR=$(PACKAGE_OUTPUT_DIR) DEB_ARCH=$(DEB_ARCH) RPM_ARCH=$(RPM_ARCH) RPM_RELEASE=$(RPM_RELEASE) scripts/check-packages.sh

package-clean:
	rm -rf $(PACKAGE_BUILD_DIR) $(RPM_TOPDIR)

clean:
	$(CARGO) clean
