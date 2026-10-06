/* OBS 共用样式适配层。只写入受控 CSS 属性，不拼接样式表或 HTML。 */
(() => {
  const properties = ['font-family', 'font-size', 'font-weight', 'text-color', 'secondary-color', 'text-shadow', 'filter-shadow'];
  window.OverlayStyle = {
    apply(value, target = document.documentElement) {
      const style = target.style;
      // 老快照没有外观字段时回到各页面 CSS 中的默认值。
      for (const property of properties) style.removeProperty(`--overlay-${property}`);
      if (!value) return;
      const set = (key, value) => style.setProperty(`--overlay-${key}`, value);
      const color = value => typeof value === 'string' && /^#[\da-f]{6}$/i.test(value);
      const number = (value, min, max) => Number.isInteger(value) && value >= min && value <= max;
      if (typeof value.font_family === 'string' && value.font_family.trim()) {
        // 按单个字体名称引用，逗号、引号和反斜杠不会变成额外 CSS 语法。
        const family = value.font_family.trim().replace(/[\\"]/g, '\\$&').replace(/[\x00-\x1f\x7f]/g, '');
        set('font-family', `"${family}", var(--overlay-default-font-family)`);
      }
      if (number(value.font_size, 12, 200)) set('font-size', `${value.font_size}px`);
      if (number(value.font_weight, 100, 900) && value.font_weight % 100 === 0) set('font-weight', String(value.font_weight));
      if (color(value.text_color)) set('text-color', value.text_color);
      if (color(value.secondary_color)) set('secondary-color', value.secondary_color);
      if (value.shadow_enabled === false) {
        set('text-shadow', 'none');
        set('filter-shadow', 'none');
      } else if (value.shadow_enabled === true && color(value.shadow_color)
        && number(value.shadow_blur, 0, 50) && number(value.shadow_offset_x, -50, 50) && number(value.shadow_offset_y, -50, 50)) {
        const shadow = `${value.shadow_offset_x}px ${value.shadow_offset_y}px ${value.shadow_blur}px ${value.shadow_color}`;
        set('text-shadow', shadow);
        // 点歌行先完成省略裁切，再绘制两层阴影，保持原有柔和边缘。
        set('filter-shadow', `drop-shadow(${shadow}) drop-shadow(0 0 ${value.shadow_blur * 2}px ${value.shadow_color}8c)`);
      }
    },
  };
})();
