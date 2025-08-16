use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect}, style::{Modifier, Style, Stylize}, text::Text, widgets::{Block, Row, Table, TableState}, Frame
};
use time::format_description;

use crate::{config, Album};

pub(crate) struct InitTui {
    pub(crate) old_albums: Vec<Album>,
    pub(crate) new_albums: Vec<Album>,
    pub(crate) new_other: Vec<Album>,
}

struct AppState {
    albums: Vec<Album>,
    other: Vec<Album>,
    table_state: TableState,
}

pub(crate) fn run(init: InitTui) -> Result<()> {
    let mut init = init;
    let mut terminal = ratatui::init();
    let mut table_state = TableState::default();
    table_state.select(Some(0));

    let mut albums = init.old_albums;
    albums.append(&mut init.new_albums);

    albums.sort_by_cached_key(|album| album.id);
    albums.dedup();
    albums.sort_by_cached_key(|album| album.date);

    let mut app_state = AppState {
        albums,
        other: init.new_other,
        table_state,
    };
    loop {
        terminal
            .draw(|frame| draw(frame, &mut app_state))
            .expect("failed to draw frame");
        match handle_events(&mut app_state) {
            InputHandling::Continue => {}
            InputHandling::Save => {
                ratatui::restore();
                let mut c = config::Config::read()?;
                c.previous = app_state.albums;
                c.now()?;
                println!("Saved!");
                return Ok(());
            }
            InputHandling::DoNotSave => {
                ratatui::restore();
                println!("We did not save");
                return Ok(());
            }
        }
    }
}

enum InputHandling {
    Continue,
    Save,
    DoNotSave,
}

fn handle_events(app_state: &mut AppState) -> InputHandling {
    match event::read().expect("Could not read") {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return InputHandling::DoNotSave,
            KeyCode::Up => app_state.table_state.select_previous(),
            KeyCode::Down => app_state.table_state.select_next(),
            KeyCode::Delete | KeyCode::Char('d') => {
                if let Some(selected) = app_state.table_state.selected() {
                    app_state.albums.remove(selected);
                }
            }
            KeyCode::Char('s') => return InputHandling::Save,
            _ => {}
        },
        _ => {}
    }
    InputHandling::Continue
}

fn album_to_row<'a>(album: &'a Album) -> Row<'a> {
    let today = time::OffsetDateTime::now_utc().date() - time::Duration::DAY;
    let format = format_description::parse("[year]-[month]-[day]").expect("Could not do format");
    let date: String = album
        .date
        .and_then(|d| d.format(&format).ok())
        .unwrap_or_else(|| "NONE".to_string());

    let style = if album.date.is_some() && album.date.unwrap() >= today {
        Style::default()
            .fg(ratatui::style::Color::Red)
            .add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default().fg(ratatui::style::Color::Green)
    };

    Row::new(vec![album.title.clone(), album.artist.clone(), date]).style(style)
}

fn construct_table<'a>(albums: &'a [Album], header: &'a str) -> Table<'a> {
    let rows = albums.iter().map(album_to_row);

    // Columns widths are constrained in the same way as Layout...
    let widths = [
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ];
    Table::new(rows, widths)
        .column_spacing(1)
        .header(
            Row::new(vec!["Title", "Artist", "Date"])
                .style(Style::new().bold())
                // To add space between the header and the rest of the rows, specify the margin
                .bottom_margin(1),
        )
        // As any other widget, a Table can be wrapped in a Block.
        .block(Block::bordered().title(header))
        // The selected row, column, cell and its content can also be styled.
        .row_highlight_style(Style::new().reversed())
        // ...and potentially show a symbol in front of the selection.
        .highlight_symbol(">>")
}

fn construct_help(frame: &mut Frame, area: Rect) {
    let layout = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
    .split(area);

    let text1 = Text::raw("- `s` to save");
    let text2 = Text::raw("- `Esc` to not save");
    frame.render_widget(text1, layout[0]);
    frame.render_widget(text2, layout[1]);
}

fn draw(frame: &mut Frame, app_state: &mut AppState) {
    let new_albums_table = construct_table(&app_state.albums, "New Albums");

    let others_table = construct_table(&app_state.other, "Others");

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(10), Constraint::Percentage(45), Constraint::Percentage(45)])
        .split(frame.area());

    construct_help(frame, layout[0]);

    frame.render_stateful_widget(new_albums_table, layout[1], &mut app_state.table_state);
    frame.render_widget(others_table, layout[2]);
}
