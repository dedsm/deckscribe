//! Description: what is asked of a vision model about a slide image. The model
//! itself is behind the `Describer` port.

/// The question sent with each slide image.
pub const PROMPT: &str = "This is a frame from a recorded presentation. \
Ignore any webcam or speaker video, burned-in captions, application toolbars and menus, and zoom indicators. \
Describe only the slide content, in this format:\n\
TITLE: the main heading, transcribed exactly (or 'None').\n\
TEXT: every other visible text, in reading order, one item per line.\n\
VISUALS: diagrams, icons, arrows and how they connect, in one or two sentences.\n\
Do not guess at anything you cannot read.";

/// Width, in pixels, of the image sent to the model.
pub const IMAGE_WIDTH: u32 = 1280;
