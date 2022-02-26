use std::{
    cell::{Ref, RefCell},
    fmt::Debug,
    fs::File,
    io::{BufReader, Error},
    ops::Add,
    path::Path,
    ptr::null,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

use super::media::Media;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum PlayStatus {
    Waiting,
    Playing(Instant, Duration), // elapsed, times already played
    Stopped(Duration),          // times already played
}

pub struct PlayListItem {
    pub name: String,
    pub duration: Duration,
    pub current_pos: Duration,
    pub status: PlayStatus,
}

pub struct PlayList {
    pub lists: Vec<PlayListItem>,
}

pub trait Player {
    // 初始化
    fn new() -> Self;

    // 添加歌曲
    fn add_to_list(&mut self, media: Media, once: bool) -> bool;

    // 播放
    fn play(&mut self) -> bool;

    // 下一首
    fn next(&mut self) -> bool;

    // 停止
    fn stop(&mut self) -> bool;

    // 暂停
    fn pause(&mut self) -> bool;

    // 继续
    fn resume(&mut self) -> bool;

    // 播放进度
    fn get_progress(&self) -> (f32, f32);

    // 是否正在播放
    fn is_playing(&self) -> bool;

    // 提供一个接口，用于更新player状态
    fn tick(&mut self);
}

pub struct MusicPlayer {
    // params
    pub current_time: Duration,
    pub total_time: Duration,
    pub play_list: PlayList,
    // media: Media,
    // stream
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    _sink: Sink,
    initialized: bool,
}

impl Player for MusicPlayer {
    fn new() -> Self {
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        Self {
            current_time: Duration::from_secs(0),
            total_time: Duration::from_secs(0),
            play_list: PlayList { lists: vec![] },
            // media: f,
            _stream: stream,
            _stream_handle: stream_handle,
            _sink: sink,
            initialized: false,
        }
    }

    fn add_to_list(&mut self, media: Media, once: bool) -> bool {
        match media.src {
            super::media::Source::Http(_) => {
                todo!();
            }
            super::media::Source::Local(path) => {
                match File::open(path.as_str()) {
                    Ok(f) => {
                        let path = Path::new(path.as_str());
                        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
                        // Result<(stream,streamHanlde),std::error:Error>
                        let mp3d = mp3_duration::from_file(&f).ok();
                        if let Some(duration) = mp3d {
                            let buf_reader = BufReader::new(f);
                            match Decoder::new(buf_reader) {
                                Ok(dec) => {
                                    if !self.initialized {
                                        self.initialized = true;
                                        self.start_listening_thread();
                                    }
                                    if once {
                                        self._sink.stop();
                                        self._sink = Sink::try_new(&self._stream_handle).unwrap();
                                        self.play_list.lists.clear();
                                    }

                                    self._sink.append(dec);

                                    // add to playlist
                                    self.play_list.lists.push(PlayListItem {
                                        name: file_name,
                                        duration: duration,
                                        current_pos: Duration::from_secs(0),
                                        status: PlayStatus::Waiting,
                                    });
                                    // start play
                                    self.play();
                                    // manually tick once
                                    self.tick();
                                    return true;
                                }
                                Err(_) => {
                                    return false;
                                }
                            }
                        } else {
                            return false;
                        }
                    }
                    Err(_) => false,
                }
            }
        }
    }

    // fn next(&mut self) -> bool {
    //     // self._sink.
    //     true
    // }

    fn play(&mut self) -> bool {
        self._sink.play();
        if let Some(item) = self.play_list.lists.first_mut() {
            let status = &mut item.status;
            match status {
                PlayStatus::Waiting => {
                    *status = PlayStatus::Playing(Instant::now(), Duration::from_nanos(0));
                }
                PlayStatus::Playing(_, _) => {}
                PlayStatus::Stopped(duration) => {
                    *status = PlayStatus::Playing(Instant::now(), *duration);
                }
            }
        }
        true
    }

    fn stop(&mut self) -> bool {
        self._sink.stop();
        true
    }

    fn pause(&mut self) -> bool {
        self._sink.pause();
        if let Some(item) = self.play_list.lists.first_mut() {
            let status = &mut item.status;
            match status {
                PlayStatus::Waiting => {}
                PlayStatus::Playing(instant, duration) => {
                    *status = PlayStatus::Stopped(duration.add(instant.elapsed()));
                }
                PlayStatus::Stopped(_) => {}
            }
        }
        true
    }

    fn resume(&mut self) -> bool {
        self._sink.play();
        if let Some(item) = self.play_list.lists.first_mut() {
            let status = &mut item.status;
            match status {
                PlayStatus::Waiting => {}
                PlayStatus::Playing(_, _) => {}
                PlayStatus::Stopped(duration) => {
                    *status = PlayStatus::Playing(Instant::now(), *duration);
                }
            }
        }
        return true;
    }

    fn is_playing(&self) -> bool {
        return self.initialized && !self._sink.is_paused() && !self.play_list.lists.is_empty();
    }

    fn get_progress(&self) -> (f32, f32) {
        return (0.0, 0.0);
    }

    fn tick(&mut self) {
        let is_playing = self.is_playing();
        if let Some(song) = self.play_list.lists.first_mut() {
            let status = &mut song.status;
            match status {
                PlayStatus::Waiting => {
                    if is_playing {
                        *status = PlayStatus::Playing(Instant::now(), Duration::from_nanos(0));
                    }
                }
                PlayStatus::Playing(instant, duration) => {
                    let now = instant.elapsed().add(duration.clone());
                    if now.ge(&song.duration) {
                        // next song, delete 0
                        self.play_list.lists.remove(0);
                    } else {
                        // update status
                        self.current_time = now;
                        self.total_time = song.duration.clone();
                    }
                }
                PlayStatus::Stopped(dur) => {
                    self.current_time = dur.clone();
                    self.total_time = song.duration.clone();
                }
            }
        } else {
            // stop player when no sounds
            if self.play_list.lists.is_empty() {
                self.stop();
            }
        }
    }

    fn next(&mut self) -> bool {
        let len = self.play_list.lists.len();
        if len > 1 {
            self.stop();
            self.play_list.lists.remove(0);
            // for
        } else {
            // no more sound to play
            return false;
        }
        true
    }
}

impl MusicPlayer {
    pub fn volume(&self) -> f32 {
        return self._sink.volume();
    }

    pub fn set_volume(&mut self, new_volume: f32) -> bool {
        self._sink.set_volume(new_volume);
        true
    }

    pub fn playing_song(&self) -> Option<&PlayListItem> {
        return self.play_list.lists.first();
    }

    fn start_listening_thread(&mut self) {
        // let mutex = Mutex::new(self);
        // let arc = Arc::new(mutex);

        // let thread_arc = arc.clone();
        // thread::spawn(move || loop {
        //     if let Ok(mp) = thread_arc.lock() {}
        // });
    }
}

impl Drop for MusicPlayer {
    fn drop(&mut self) {
        // println!()
    }
}
