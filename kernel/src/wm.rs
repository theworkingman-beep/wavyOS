#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficLight {
    Close,
    Minimize,
    Maximize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    None,
    DockIcon(usize),
    TrafficLight(TrafficLight),
    TitleBar,
    WindowBody,
}

pub struct DesktopLayout {
    pub win_x: u16,
    pub win_y: u16,
    pub win_w: u16,
    pub win_h: u16,
    pub dock_y: u16,
    pub dock_x: u16,
    pub dock_w: u16,
}

pub fn hit_test(x: u16, y: u16, layout: &DesktopLayout) -> HitTarget {
    let title_h: u16 = 36;

    let light_y = layout.win_y + 12;
    if circle_hit(x, y, layout.win_x + 16, light_y + 6, 6) {
        return HitTarget::TrafficLight(TrafficLight::Close);
    }
    if circle_hit(x, y, layout.win_x + 32, light_y + 6, 6) {
        return HitTarget::TrafficLight(TrafficLight::Minimize);
    }
    if circle_hit(x, y, layout.win_x + 48, light_y + 6, 6) {
        return HitTarget::TrafficLight(TrafficLight::Maximize);
    }

    if x >= layout.win_x + 60
        && x < layout.win_x + layout.win_w
        && y >= layout.win_y
        && y < layout.win_y + title_h
    {
        return HitTarget::TitleBar;
    }

    let icon_size: u16 = 48;
    let icon_spacing: u16 = 12;
    let icons = 5;
    let start_x = layout.dock_x + 16;

    if y >= layout.dock_y && y < layout.dock_y + 70 {
        for i in 0..icons {
            let ix = start_x + i * (icon_size + icon_spacing);
            if x >= ix && x < ix + icon_size {
                return HitTarget::DockIcon(i as usize);
            }
        }
    }

    if x >= layout.win_x
        && x < layout.win_x + layout.win_w
        && y >= layout.win_y + title_h
        && y < layout.win_y + layout.win_h
    {
        return HitTarget::WindowBody;
    }

    HitTarget::None
}

fn circle_hit(px: u16, py: u16, cx: u16, cy: u16, r: u16) -> bool {
    let dx = (px as i32 - cx as i32).abs();
    let dy = (py as i32 - cy as i32).abs();
    (dx * dx + dy * dy) <= (r as i32 * r as i32)
}
