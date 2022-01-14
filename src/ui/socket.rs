use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::env;
use std::thread;
use std::path::{Path, PathBuf};
use tungstenite::{Message, WebSocket, accept};
use serde_json::{Value, json};
use serde::{Serialize, Deserialize};

use crate::tag::{TagChanges, TagSeparators};
use crate::tagger::{TaggerConfig, Tagger};
use crate::tagger::spotify::Spotify;
use crate::ui::{Settings, StartContext};
use crate::ui::player::{AudioSources, AudioPlayer};
use crate::ui::quicktag::{QuickTag, QuickTagFile};
use crate::ui::audiofeatures::{AudioFeaturesConfig, AudioFeatures};
use crate::ui::tageditor::TagEditor;
use crate::ui::browser::FileBrowser;
use crate::playlist::{UIPlaylist, PLAYLIST_EXTENSIONS, get_files_from_playlist_file};


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum Action {
    Init,
    SaveSettings { settings: Value },
    LoadSettings,
    Browse { path: Option<String>, context: Option<String> },
    Browser { url: String },
    OpenSettingsFolder,
    OpenFolder { path: String },

    StartTagging { config: TaggerConfigs, playlist: Option<UIPlaylist> },
    
    Waveform { path: String },
    PlayerLoad { path: String },
    PlayerPlay, 
    PlayerPause,
    PlayerSeek { pos: u64 },
    PlayerVolume { volume: f32 },

    QuickTagLoad { path: Option<String>, playlist: Option<UIPlaylist>, recursive: Option<bool>, separators: TagSeparators },
    QuickTagSave { changes: TagChanges },
    QuickTagFolder { path: Option<String>, subdir: Option<String> },

    #[serde(rename_all = "camelCase")]
    SpotifyAuthorize { client_id: String, client_secret: String },
    SpotifyAuthorized,

    TagEditorFolder { path: Option<String>, subdir: Option<String>, recursive: Option<bool>  },
    TagEditorLoad { path: String },
    TagEditorSave { changes: TagChanges }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum TaggerConfigs {
    AutoTagger(TaggerConfig), 
    AudioFeatures(AudioFeaturesConfig)
}

impl TaggerConfigs {
    // Print to log for later easier debug
    pub fn debug_print(&self) {
        match self {
            TaggerConfigs::AutoTagger(c) => {
                let mut c = c.clone();
                c.discogs.token = None;
                c.spotify = None;
                info!("AutoTagger config: {:?}", c);
            },
            TaggerConfigs::AudioFeatures(c) => {
                info!("AudioFeatures Config: {:?}", c);
            }
        }
    }
}

// Shared variables in socket
struct SocketContext {
    player: AudioPlayer,
    spotify: Option<Spotify>,
    start_context: StartContext
} 

impl SocketContext {
    pub fn new(start_context: StartContext) -> SocketContext {
        SocketContext {
            player: AudioPlayer::new(),
            spotify: None,
            start_context
        }
    }
}


// Start WebSocket UI server
pub fn start_socket_server(context: StartContext) {
    let host = match context.expose {
        true => "0.0.0.0:36912",
        false => "127.0.0.1:36912"
    };
    let server = TcpListener::bind(host).unwrap();
    for stream in server.incoming() {
        let context = context.clone();
        thread::spawn(move || {
            // Create shared
            let mut context = SocketContext::new(context);

            // Websocket loop
            let mut websocket = accept(stream.unwrap()).unwrap();
            loop {
                match websocket.read_message() {
                    Ok(msg) => {
                        if msg.is_text() {
                            let text = msg.to_text().unwrap();
                            match handle_message(text, &mut websocket, &mut context) {
                                Ok(_) => {},
                                Err(err) => {
                                    // Send error to UI
                                    error!("Websocket: {:?}, Data: {}", err, text);
                                    websocket.write_message(Message::from(json!({
                                        "action": "error",
                                        "message": &format!("{}", err)
                                    }).to_string())).ok();
                                }
                            }
                        }
                    },
                    Err(e) => {
                        // Connection closed
                        if !websocket.can_read() || !websocket.can_write() {
                            warn!("{} - Websocket can't read or write, closing connection!", e);
                            break;
                        }
                        warn!("Invalid websocket message: {}", e);
                    }
                }
            }
        });
    }
}


fn handle_message(text: &str, websocket: &mut WebSocket<TcpStream>, context: &mut SocketContext) -> Result<(), Box<dyn Error>> {
    // Parse JSON
    let action: Action = serde_json::from_str(text)?;
    match action {
        // Get initial info
        Action::Init => {
            websocket.write_message(Message::from(json!({
                "action": "init",
                "version": crate::VERSION,
                "os": env::consts::OS,
                "startContext": context.start_context
            }).to_string())).ok();
        },
        Action::SaveSettings { settings } => Settings::from_ui(&settings).save()?,
        Action::LoadSettings => match Settings::load() {
            Ok(settings) => {
                websocket.write_message(Message::from(json!({
                    "action": "loadSettings",
                    "settings": settings.ui
                }).to_string())).ok();
            }
            // Ignore settings if they don't exist (might be initial load)
            Err(e) => error!("Failed loading settings, using defaults. {}", e)
        },
        // Browse for folder
        Action::Browse { path, context } => {
            let mut initial = path.unwrap_or(".".to_string());
            if initial.is_empty() || !Path::new(&initial).exists() {
                initial = ".".to_string()
            }
            if let Some(path) = tinyfiledialogs::select_folder_dialog("Select path", &initial) {
                websocket.write_message(Message::from(json!({
                    "action": "browse",
                    "path": path,
                    "context": context
                }).to_string())).ok();
            }
        },
        // Open URL in external browser
        Action::Browser { url } => { webbrowser::open(&url)?; },
        Action::OpenSettingsFolder => opener::open(Settings::get_folder()?.to_str().unwrap())?,
        Action::OpenFolder { path } => { opener::open(&path).ok(); },
        Action::StartTagging { config, playlist } => {
            config.debug_print();

            // Load playlist
            let mut files = if let Some(playlist) = playlist {
                playlist.get_files()?
            } else { vec![] };
            let mut file_count = files.len();
            let mut folder_path = None;
            // Load taggers
            let (tagger_type, rx) = match config {
                TaggerConfigs::AutoTagger(c) => {
                    // Load file list
                    if files.is_empty() {
                        let path = c.path.as_ref().map(|p| p.to_owned()).unwrap_or(String::new());
                        files = Tagger::get_file_list(&path);
                        file_count = files.len();
                        folder_path = Some(path);
                    }
                    let rx = Tagger::tag_files(&c, files);
                    ("autoTagger", rx)
                },
                TaggerConfigs::AudioFeatures(c) => {
                    if files.is_empty() {
                        let path = c.path.as_ref().unwrap_or(&String::new()).to_owned();
                        files = Tagger::get_file_list(&path);
                        folder_path = Some(path);
                        file_count = files.len();
                    }
                    // Authorize spotify
                    let spotify = context.spotify.as_ref().ok_or("Spotify unauthorized!")?.to_owned().to_owned();
                    let rx = AudioFeatures::start_tagging(c.clone(), spotify, files);
                    ("audioFeatures", rx)
                },
            };

            // Start
            websocket.write_message(Message::from(json!({
                "action": "startTagging",
                "files": file_count,
                "type": tagger_type
            }).to_string())).ok();
            // Tagging
            let start = timestamp!();
            for status in rx {
                websocket.write_message(Message::from(json!({
                    "action": "taggingProgress",
                    "status": status
                }).to_string())).ok();
            }
            info!("Tagging finished, took: {} seconds.", (timestamp!() - start) / 1000);
            // Done
            websocket.write_message(Message::from(json!({
                "action": "taggingDone",
                "path": folder_path
            }).to_string())).ok();
        },
        Action::Waveform { path } => {
            let source = AudioSources::from_path(&path)?;
            let (waveform_rx, cancel_tx) = source.generate_waveform(180)?;
            // Streamed
            for wave in waveform_rx {
                websocket.write_message(Message::from(json!({
                    "action": "waveformWave",
                    "wave": wave
                }).to_string())).ok();
                // Check reply
                websocket.read_message().ok();
                if !websocket.can_write() {
                    cancel_tx.send(true).ok();
                }
            }
            // Done
            websocket.write_message(Message::from(json!({
                "action": "waveformDone",
            }).to_string())).ok();
        },
        // Load player file
        Action::PlayerLoad { path } => {
            let source = AudioSources::from_path(&path)?;
            // Send to UI
            websocket.write_message(Message::from(json!({
                "action": "playerLoad",
                "duration": source.duration() as u64
            }).to_string())).ok();
            // Load
            context.player.load_file(source);
        },
        //  Controls
        Action::PlayerPlay => context.player.play(),
        Action::PlayerPause => context.player.pause(),
        Action::PlayerSeek { pos } => {
            websocket.write_message(Message::from(json!({
                "action": "playerSync",
                "playing": context.player.seek(pos)
            }).to_string())).ok();
        },
        Action::PlayerVolume { volume } => context.player.volume(volume),
        // Load quicktag files or playlist
        Action::QuickTagLoad { path, playlist, recursive, separators } => {
            let mut files = vec![];
            // Playlist
            if let Some(playlist) = playlist {
                files = QuickTag::load_files_playlist(&playlist, &separators)?;
            }
            // Path
            if let Some(path) = path {
                if PLAYLIST_EXTENSIONS.iter().any(|e| path.to_lowercase().ends_with(e)) {
                    files = QuickTag::load_files(get_files_from_playlist_file(&path)?, &separators)?;
                } else {
                    files = QuickTag::load_files_path(&path, recursive.unwrap_or(false), &separators)?;
                }
            }
            websocket.write_message(Message::from(json!({
                "action": "quickTagLoad",
                "data": files
            }).to_string())).ok();
        },
        // Save quicktag changes
        Action::QuickTagSave { changes } => {
            let tag = changes.commit()?;
            websocket.write_message(Message::from(json!({
                "action": "quickTagSaved",
                "path": &changes.path,
                "file": QuickTagFile::from_tag(&changes.path, &tag).ok_or("Failed loading tags")?
            }).to_string())).ok();
        },
        // List dir
        Action::QuickTagFolder { path, subdir } => {
            let (new_path, files) = FileBrowser::list_dir_or_default(path.clone().map(|p| PathBuf::from(p)), subdir, true, false, false)?;
            websocket.write_message(Message::from(json!({
                "action": "quickTagFolder",
                "files": files,
                "path": new_path,
            }).to_string())).ok();
        }
        Action::SpotifyAuthorize { client_id, client_secret } => {
            // Authorize cached
            if let Some(spotify) = Spotify::try_cached_token(&client_id, &client_secret) {
                context.spotify = Some(spotify);
            // Authorize new
            } else {
                let (auth_url, mut oauth) = Spotify::generate_auth_url(&client_id, &client_secret);
                webbrowser::open(&auth_url)?;
                let spotify = Spotify::auth_server(&mut oauth, context.start_context.expose)?;
                context.spotify = Some(spotify);
            }
            websocket.write_message(Message::from(json!({
                "action": "spotifyAuthorized",
                "value": true
            }).to_string())).ok();
        },
        // Check if authorized
        Action::SpotifyAuthorized => {
            websocket.write_message(Message::from(json!({
                "action": "spotifyAuthorized",
                "value": context.spotify.is_some()
            }).to_string())).ok();
        },
        Action::TagEditorFolder { path, subdir, recursive } => {
            let recursive = recursive.unwrap_or(false);
            let (new_path, files) = FileBrowser::list_dir_or_default(path.clone().map(|p| PathBuf::from(p)), subdir, true, true, recursive)?;
            websocket.write_message(Message::from(json!({
                "action": "tagEditorFolder",
                "files": files,
                "path": new_path,
                // Stateless
                "recursive": recursive
            }).to_string())).ok();
        },
        // Load tags of file
        Action::TagEditorLoad { path } => {
            let data = TagEditor::load_file(&path)?;
            websocket.write_message(Message::from(json!({
                "action": "tagEditorLoad",
                "data": data
            }).to_string())).ok();
        },
        // Save changes
        Action::TagEditorSave { changes } => {
            let _tag = changes.commit()?;
            websocket.write_message(Message::from(json!({
                "action": "tagEditorSave"
            }).to_string())).ok();
        },
    }
   
    Ok(())
}