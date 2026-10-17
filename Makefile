.PHONY: build install uninstall

build:
	cargo build --release --locked --features system-helper --bins

# Run build as a normal user first. DESTDIR supports packaging without installation.
install:
	install -Dm755 target/release/pacstore "$(DESTDIR)/usr/bin/pacstore"
	install -Dm755 target/release/pacstore-helper "$(DESTDIR)/usr/lib/pacstore/pacstore-helper"
	install -Dm644 packaging/org.pacstore.transactions.policy "$(DESTDIR)/usr/share/polkit-1/actions/org.pacstore.transactions.policy"

uninstall:
	rm -f "$(DESTDIR)/usr/bin/pacstore" "$(DESTDIR)/usr/lib/pacstore/pacstore-helper" "$(DESTDIR)/usr/share/polkit-1/actions/org.pacstore.transactions.policy"
