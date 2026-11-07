PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
APPDIR  ?= $(DATADIR)/applications
ICONDIR ?= $(DATADIR)/icons/hicolor

CARGO   ?= cargo
DESTDIR ?=
BIN     := target/release/hypredit

.PHONY: all build release debug install uninstall clean test help

all: release

build: release

release:
	$(CARGO) build --release

debug:
	$(CARGO) build

test:
	$(CARGO) test

install: release
	install -Dm755 $(BIN) $(DESTDIR)$(BINDIR)/hypredit
	install -Dm644 packaging/hypredit.desktop $(DESTDIR)$(APPDIR)/hypredit.desktop
	install -Dm644 logo.png $(DESTDIR)$(ICONDIR)/512x512/apps/hypredit.png

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/hypredit
	rm -f $(DESTDIR)$(APPDIR)/hypredit.desktop
	rm -f $(DESTDIR)$(ICONDIR)/512x512/apps/hypredit.png

clean:
	$(CARGO) clean

help:
	@echo "Targets:"
	@echo "  make            build release binary"
	@echo "  make debug      build debug binary"
	@echo "  make test       run unit tests"
	@echo "  make install    install to PREFIX (default /usr/local)"
	@echo "  make uninstall  remove installed files"
	@echo "  make clean      remove build artifacts"
