.PHONY: fmt test check clippy verify lm-2026-podium

CRAWL_FOCUS_CODE ?= OD
PODIUM_FOCUS_CODE ?= all
LM_2026_URL := https://www.kschv-rdeck.de/fileadmin/user_upload/ksv/NDSB_TEMP/LM-Dinge/Ergebnisse/LM-Ergebnisslisten2026.html
LM_2026_EXTRA_PDF := https://www.kschv-rdeck.de/fileadmin/user_upload/ksv/NDSB_TEMP/LM-Dinge/Ergebnisse/VW112_K40_260516_1045_Finale_10.pdf
LM_2026_REPORT := reports/archive/2026/landesmeisterschaften/crawl-report.json
LM_2026_PODIUM_JSON := reports/archive/2026/landesmeisterschaften/podium-export.json
LM_2026_PODIUM_HTML := reports/archive/2026/landesmeisterschaften/podium-export.html

fmt:
	cargo fmt

test:
	cargo test

check:
	cargo check

clippy:
	cargo clippy --all-targets --all-features -- -W clippy::pedantic -W clippy::nursery -D warnings

verify: fmt test check clippy

lm-2026-podium:
	cargo run -- crawl-report "$(LM_2026_URL)" --source-name landesmeisterschaften --year 2026 --focus Stormarn --focus-association-code "$(CRAWL_FOCUS_CODE)" --extra-pdf-url "$(LM_2026_EXTRA_PDF)"
	cargo run -- export-podium --crawl-report "$(LM_2026_REPORT)" --output "$(LM_2026_PODIUM_JSON)" --html-output "$(LM_2026_PODIUM_HTML)" --focus-association-code "$(PODIUM_FOCUS_CODE)" --max-place 3
