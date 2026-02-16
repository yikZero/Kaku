pub const OPENCODE_THEME_JSON: &str = r##"{
  "$schema": "https://opencode.ai/theme.json",
  "defs": {
    "bg": "#15141b",
    "panel": "#15141b",
    "element": "#1f1d28",
    "text": "#edecee",
    "muted": "#6b6b6b",
    "primary": "#a277ff",
    "secondary": "#61ffca",
    "accent": "#ffca85",
    "error": "#ff6767",
    "warning": "#ffca85",
    "success": "#61ffca",
    "info": "#a277ff",
    "border": "#15141b",
    "border_active": "#29263c",
    "border_subtle": "#15141b"
  },
  "theme": {
    "primary": "primary",
    "secondary": "secondary",
    "accent": "accent",
    "error": "error",
    "warning": "warning",
    "success": "success",
    "info": "info",
    "text": "text",
    "textMuted": "muted",
    "background": "bg",
    "backgroundPanel": "panel",
    "backgroundElement": "element",
    "border": "border",
    "borderActive": "border_active",
    "borderSubtle": "border_subtle",
    "diffAdded": "success",
    "diffRemoved": "error",
    "diffContext": "muted",
    "diffHunkHeader": "primary",
    "diffHighlightAdded": "success",
    "diffHighlightRemoved": "error",
    "diffAddedBg": "#1b2a24",
    "diffRemovedBg": "#2a1b20",
    "diffContextBg": "bg",
    "diffLineNumber": "muted",
    "diffAddedLineNumberBg": "#1b2a24",
    "diffRemovedLineNumberBg": "#2a1b20",
    "markdownText": "text",
    "markdownHeading": "primary",
    "markdownLink": "info",
    "markdownLinkText": "primary",
    "markdownCode": "accent",
    "markdownBlockQuote": "muted",
    "markdownEmph": "accent",
    "markdownStrong": "secondary",
    "markdownHorizontalRule": "muted",
    "markdownListItem": "primary",
    "markdownListEnumeration": "accent",
    "markdownImage": "info",
    "markdownImageText": "primary",
    "markdownCodeBlock": "text",
    "syntaxComment": "muted",
    "syntaxKeyword": "primary",
    "syntaxFunction": "secondary",
    "syntaxVariable": "text",
    "syntaxString": "success",
    "syntaxNumber": "accent",
    "syntaxType": "info",
    "syntaxOperator": "primary",
    "syntaxPunctuation": "text"
  }
}
"##;

mod tui;

use anyhow::Context;
use clap::Parser;

#[derive(Debug, Parser, Clone, Default)]
pub struct AiConfigCommand {}

impl AiConfigCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        tui::run().context("ai config tui")
    }
}
