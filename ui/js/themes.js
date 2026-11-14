// Phong cách văn phòng: bảng màu theo tên vật liệu Kenney (wood, carpet, metal…), ánh sáng và đồ trang trí.

export const THEMES = {
  hien_dai: {
    name: 'Hiện đại',
    desc: 'Sáng sủa, tông kem và gỗ sáng, nhiều ánh sáng tự nhiên.',
    swatch: ['#efe4d2', '#f4f1ea', '#e6b17e', '#f05f57'],
    floor: '#eadfcb', floorAlt: '#e2d4bc', wall: '#f4f1ea', trim: '#cdb79a',
    wood: '#e6b17e', woodDark: '#ad7550', fabric: '#f05f57', fabricDark: '#9c4c4a', fabric2: '#5c85de', metal: '#bdd1d6',
    bg: '#141a33', slab: '#2b3357', hemiSky: '#e8eeff', hemiGround: '#8a7c6a', hemi: 1.35, sun: '#fff3e0', sunI: 2.4, exposure: 1.05,
    sofa: 'loungeSofa', bookcase: 'bookcaseClosedWide', plants: 4,
  },
  go_am: {
    name: 'Gỗ ấm cúng',
    desc: 'Sàn gỗ nâu, ánh đèn vàng, nhiều cây xanh và kệ sách.',
    swatch: ['#a8703f', '#efe0c4', '#7a4b2a', '#c8553d'],
    floor: '#a8703f', floorAlt: '#9b6536', wall: '#efe0c4', trim: '#7a4b2a',
    wood: '#b9824f', woodDark: '#7a4b2a', fabric: '#c8553d', fabricDark: '#8a3b2b', fabric2: '#d9a05b', metal: '#c9b79c',
    bg: '#1d130c', slab: '#3b2a1e', hemiSky: '#ffe7c7', hemiGround: '#6b4a2f', hemi: 1.2, sun: '#ffd49a', sunI: 2.2, exposure: 1.0,
    sofa: 'loungeSofa', bookcase: 'bookcaseOpen', plants: 8, lamps: true,
  },
  cong_nghe: {
    name: 'Công nghệ (tối)',
    desc: 'Tông tối, đèn neon xanh tím, màn hình phát sáng.',
    swatch: ['#2b3150', '#1d2238', '#22d3ee', '#a78bfa'],
    floor: '#3a4270', floorAlt: '#333a63', wall: '#2a3154', trim: '#4a5590',
    wood: '#3a4160', woodDark: '#262b45', fabric: '#22d3ee', fabricDark: '#0e7490', fabric2: '#a78bfa', metal: '#94a3b8',
    bg: '#04060d', slab: '#151a35', hemiSky: '#a5b4ff', hemiGround: '#262b4d', hemi: 1.05, sun: '#c7d2fe', sunI: 1.8, exposure: 1.2,
    sofa: 'loungeDesignSofa', bookcase: 'bookcaseClosedWide', plants: 2, neon: ['#22d3ee', '#a78bfa', '#f472b6'], screenGlow: 0.35,
  },
  sang_tao: {
    name: 'Startup sáng tạo',
    desc: 'Màu pastel tươi vui, sofa thiết kế, thảm nhiều màu.',
    swatch: ['#d7efe9', '#ffe1cc', '#ff7eb6', '#4fc3f7'],
    floor: '#d6eee8', floorAlt: '#c8e6de', wall: '#ffe1cc', trim: '#ff9f7a',
    wood: '#f2c38f', woodDark: '#d98f5a', fabric: '#ff7eb6', fabricDark: '#c2185b', fabric2: '#4fc3f7', metal: '#e5e7eb',
    bg: '#1b1035', slab: '#3b2a63', hemiSky: '#fff1f7', hemiGround: '#7c6aa6', hemi: 1.4, sun: '#fff7ed', sunI: 2.5, exposure: 1.05,
    sofa: 'loungeDesignSofa', bookcase: 'bookcaseOpen', plants: 6,
  },
  hanh_chinh: {
    name: 'Hành chính trang nghiêm',
    desc: 'Thảm xám xanh, bàn gỗ sẫm, tủ hồ sơ đóng cửa.',
    swatch: ['#8c98ab', '#f3efe6', '#7b5232', '#2f4a7a'],
    floor: '#8d99ac', floorAlt: '#8692a6', wall: '#f3efe6', trim: '#6b4a2f',
    wood: '#7b5232', woodDark: '#4e321e', fabric: '#2f4a7a', fabricDark: '#1f3157', fabric2: '#2f4a7a', metal: '#b8b8b8',
    bg: '#0f141e', slab: '#262f40', hemiSky: '#eef2ff', hemiGround: '#5d6676', hemi: 1.3, sun: '#fffaf0', sunI: 2.3, exposure: 1.0,
    sofa: 'loungeSofa', bookcase: 'bookcaseClosedDoors', plants: 4,
  },
};
export const DEFAULT_THEME = 'hien_dai';
export const themeOf = (key) => THEMES[key] || THEMES[DEFAULT_THEME];

/** Ánh xạ tên vật liệu → màu theo phong cách. */
export function paletteFor(theme, role) {
  if (role === 'floor') return { wood: theme.floor };
  if (role === 'floorAlt') return { wood: theme.floorAlt };
  if (role === 'wall') return { _defaultMat: theme.wall, wood: theme.trim };
  return {
    wood: theme.wood, woodDark: theme.woodDark, carpet: theme.fabric, carpetDarker: theme.fabricDark,
    carpetBlue: theme.fabric2, metal: theme.metal,
  };
}
