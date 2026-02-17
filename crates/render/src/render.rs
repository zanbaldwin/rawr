use crate::error::{ErrorKind, Result};
use crate::{Renderer, TempFile, style::CssVariables};
use exn::ResultExt;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use tracing::instrument;

pub enum Output {
    Persisted(PathBuf),
    Temporary(TempFile),
}

impl Renderer {
    pub fn render<R: Read>(&self, html: R, variables: impl Into<Option<CssVariables>>) -> Result<Output> {
        let output = TempFile::new().or_raise(|| ErrorKind::Io)?;
        self.render_to(html, variables, output.path().to_path_buf())?;
        Ok(Output::Temporary(output))
    }

    #[instrument(skip_all)]
    pub fn render_to<R: Read>(
        &self,
        html: R,
        variables: impl Into<Option<CssVariables>>,
        save_to: impl Into<PathBuf>,
    ) -> Result<Output> {
        let save_to = save_to.into();
        let input = self.persist_html(html, variables.into())?;
        self.chrome.execute(input.path(), &save_to)?;
        Ok(Output::Persisted(save_to))
    }

    pub fn render_slice(&self, html: &[u8], variables: impl Into<Option<CssVariables>>) -> Result<Output> {
        self.render(Cursor::new(html), variables)
    }

    pub fn render_slice_to(
        &self,
        html: &[u8],
        variables: impl Into<Option<CssVariables>>,
        save_to: impl Into<PathBuf>,
    ) -> Result<Output> {
        self.render_to(Cursor::new(html), variables, save_to)
    }

    fn persist_html<R: Read>(&self, mut html: R, variables: Option<CssVariables>) -> Result<TempFile> {
        let mut tmp = TempFile::new().or_raise(|| ErrorKind::Io)?;
        const NEEDLE: &[u8] = b"</head";
        const CARRY_SIZE: usize = NEEDLE.len() - 1;
        // We want to load 2 pages into memory each time.
        const BUFFER_CAPACITY: usize = 8192;
        const BUFFER_WINDOW: usize = BUFFER_CAPACITY - CARRY_SIZE;
        // All because I REFUSE to read the entire file into memory... tsk tsk.
        // Assuming NEEDLE=`123` + BUFFER_CAPACITY=12 (CARRY_SIZE=2, BUFFER_WINDOW=10):
        //
        // First loop:   BBBBBBBBbb...................123.......... carry=0, buf[0..10], bytes=10, filled=10, safe=8,  (consumed  0..10)
        //               \--buffer--/
        // Second loop:  rrrrrrrrCCBBBBBBBBbb.........123.......... carry=2, buf[2..12], bytes=10, filled=12, safe=10, (consumed 10..20)
        //                       \--buffer--/
        // Third loop:   rrrrrrrrrrrrrrrrrrCCBBBBBBBBbb23.......... carry=2, buf[2..12], bytes=10, filled=12, safe=10, (consumed 20..30)
        //                                 \--buffer--/
        // Fourth loop:  rrrrrrrrrrrrrrrrrrrrrrrrrrrrC123---------- carry=2, buf[2..12], bytes=10, filled=12, write_all + io::copy.
        //                                           \--buffer--/
        let mut buffer = vec![0; BUFFER_WINDOW + CARRY_SIZE];
        let mut carry: usize = 0;
        'chunk: loop {
            let bytes = html.read(&mut buffer[carry..carry + BUFFER_WINDOW]).or_raise(|| ErrorKind::Io)?;
            if bytes == 0 {
                tmp.write_all(&buffer[..carry]).or_raise(|| ErrorKind::Io)?;
                break 'chunk;
            }
            // Well, kinda. We've consumed $bytes. But we overlap each time by CARRY_SIZE.
            let filled = carry + bytes;
            if let Some(pos) = buffer[..filled].windows(NEEDLE.len()).position(|w| w.eq_ignore_ascii_case(NEEDLE)) {
                tmp.write_all(&buffer[..pos]).or_raise(|| ErrorKind::Io)?;
                let blocks = self.inject_css(&mut tmp, variables)?;
                tmp.write_all(&buffer[pos..filled]).or_raise(|| ErrorKind::Io)?;
                tracing::debug!(position = pos, blocks = blocks, "Custom CSS stylesheets injected into HTML");
                std::io::copy(&mut html, &mut tmp).or_raise(|| ErrorKind::Io)?;
                return Ok(tmp);
            }
            let safe = filled.saturating_sub(CARRY_SIZE);
            tmp.write_all(&buffer[..safe]).or_raise(|| ErrorKind::Io)?;
            buffer.copy_within(safe..filled, 0);
            carry = filled - safe;
        }
        tracing::warn!("Custom CSS stylesheets not injected; closing head tag not found");
        Ok(tmp)
    }

    fn inject_css(&self, w: &mut impl Write, variables: Option<CssVariables>) -> Result<usize> {
        if let Some(vars) = &variables {
            write!(w, "{}", vars).or_raise(|| ErrorKind::Io)?;
        }
        let blocks = self.styles.write_all_to(w).or_raise(|| ErrorKind::Io)?;
        let blocks = if variables.is_some() { blocks.saturating_add(1) } else { blocks };
        Ok(blocks)
    }
}
