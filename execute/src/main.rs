use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{
        self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
        disable_raw_mode, enable_raw_mode,
    },
};
use rand::Rng;
use std::io::{stdout, Write};
use std::time::Duration;

const MSG: &str = "hello, world";
const FRAME_MS: u64 = 80; // 프레임 간격 (ms) — 낮을수록 빠름, 높을수록 CPU 절약

// 화면에 그려질 한 칸의 상태 (문자 + 색상)
#[derive(Clone, PartialEq)]
struct Cell {
    ch: char,
    color: Color,
}

// 한 열(column)을 따라 내려오는 빗방울 한 줄기
struct Drop {
    x: u16,        // 어느 열에 위치하는지
    head: f32,     // 머리(가장 아래) y 좌표 (float: 속도를 소수점으로 표현)
    speed: f32,    // 프레임당 내려가는 속도
    trail_len: u16,// 꼬리 길이
    chars: Vec<char>, // 꼬리를 채우는 랜덤 문자들
}

impl Drop {
    fn new(x: u16, rows: u16, rng: &mut impl Rng) -> Self {
        let trail_len = rng.gen_range(4u16..10);
        // 시작 위치를 화면 위쪽 임의 지점으로 분산 → 한꺼번에 내려오지 않게
        let head = -(rng.gen_range(0u16..rows) as f32);
        let chars = (0..trail_len).map(|_| random_char(rng)).collect();
        Drop { x, head, speed: rng.gen_range(0.156f32..0.624), trail_len, chars }
    }

    // 매 프레임마다 위치 갱신 + 화면 밖으로 나가면 재활용
    fn step(&mut self, rows: u16, rng: &mut impl Rng) {
        self.head += self.speed;

        // 일부 문자를 랜덤하게 교체 → 글리치(glitch) 느낌
        if rng.gen_bool(0.15) {
            let i = rng.gen_range(0..self.chars.len());
            self.chars[i] = random_char(rng);
        }

        // 꼬리까지 화면 아래로 완전히 벗어나면 맨 위에서 다시 시작
        if self.head as i32 - self.trail_len as i32 > rows as i32 {
            self.head = -(rng.gen_range(0u16..rows) as f32);
            self.speed = rng.gen_range(0.156f32..0.624);
            for c in &mut self.chars {
                *c = random_char(rng);
            }
        }
    }

    // 이 Drop이 차지하는 셀들을 버퍼에 기록
    fn render(&self, buf: &mut Vec<Vec<Option<Cell>>>, rows: u16) {
        let head_y = self.head as i32;
        for i in 0..self.trail_len as i32 {
            let y = head_y - i;
            if y < 0 || y >= rows as i32 {
                continue; // 화면 밖은 건너뜀
            }
            // 머리에서 멀수록 어두운 초록색으로 → 페이드 아웃 효과
            let color = match i {
                0 => Color::White,      // 머리: 흰색 (가장 밝음)
                1..=2 => Color::Green,  // 바로 뒤: 밝은 초록
                _ => Color::DarkGreen,  // 꼬리: 어두운 초록
            };
            let ch = self.chars[i as usize % self.chars.len()];
            buf[y as usize][self.x as usize] = Some(Cell { ch, color });
        }
    }
}

// 랜덤 문자 선택 (ASCII 기호 + 반각 카타카나로 Matrix 느낌)
fn random_char(rng: &mut impl Rng) -> char {
    const POOL: &str =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz\
         0123456789@#$%&*+=-<>[]{}|/\\";
    let chars: Vec<char> = POOL.chars().collect();
    chars[rng.gen_range(0..chars.len())]
}

fn main() -> std::io::Result<()> {
    let mut stdout = stdout();

    // raw mode: 키 입력을 버퍼링 없이 즉시 읽기 위해 필요
    enable_raw_mode()?;
    // alternate screen: 기존 터미널 내용을 보존하고 종료 시 복구
    execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    let (cols, rows) = terminal::size()?;
    let mut rng = rand::thread_rng();

    // 2열마다 Drop 하나씩 생성 (절반 밀도)
    let mut drops: Vec<Drop> = (0..cols).step_by(2).map(|x| Drop::new(x, rows, &mut rng)).collect();

    // 이전 프레임 버퍼 — 변경된 셀만 다시 그려 깜빡임(flickering) 방지
    let mut prev_buf: Vec<Vec<Option<Cell>>> = vec![vec![None; cols as usize]; rows as usize];

    // "hello, world" 고정 위치 (화면 정중앙)
    let msg_x = (cols.saturating_sub(MSG.len() as u16)) / 2;
    let msg_y = rows / 2;

    loop {
        // 논블로킹 키 입력 감지
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    // q, Esc, Ctrl+C 로 종료
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    _ => {}
                }
            }
        }

        // ── 새 프레임 버퍼 구성 ──────────────────────────────────
        let mut buf: Vec<Vec<Option<Cell>>> = vec![vec![None; cols as usize]; rows as usize];

        // 1) 모든 Drop 렌더링
        for drop in &drops {
            drop.render(&mut buf, rows);
        }

        // 2) "hello, world"를 노란색으로 항상 최상단에 덮어씌움
        for (i, ch) in MSG.chars().enumerate() {
            let cx = msg_x + i as u16;
            if cx < cols {
                buf[msg_y as usize][cx as usize] = Some(Cell { ch, color: Color::Red });
            }
        }

        // ── 변경된 셀만 터미널에 출력 (diff 렌더링) ────────────────
        for y in 0..rows as usize {
            for x in 0..cols as usize {
                if buf[y][x] == prev_buf[y][x] {
                    continue; // 이전과 동일하면 스킵
                }
                queue!(stdout, MoveTo(x as u16, y as u16))?;
                match &buf[y][x] {
                    Some(cell) => {
                        queue!(stdout, SetForegroundColor(cell.color), Print(cell.ch))?;
                    }
                    None => {
                        // 이전에 뭔가 있었지만 이번엔 없음 → 빈칸으로 지움
                        queue!(stdout, ResetColor, Print(' '))?;
                    }
                }
            }
        }
        stdout.flush()?;
        prev_buf = buf;

        // 모든 Drop 위치 갱신
        for drop in &mut drops {
            drop.step(rows, &mut rng);
        }

        std::thread::sleep(Duration::from_millis(FRAME_MS));
    }

    // 터미널 원상복구 (색상, 화면, 커서)
    execute!(stdout, ResetColor, LeaveAlternateScreen, Show)?;
    disable_raw_mode()?;
    Ok(())
}
