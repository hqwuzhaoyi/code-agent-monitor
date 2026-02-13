//! 搜索输入组件

/// 搜索输入组件，支持光标移动和文本编辑
#[derive(Debug, Clone, Default)]
pub struct SearchInput {
    /// 输入缓冲区
    buffer: String,
    /// 光标位置（字符索引，非字节）
    cursor_pos: usize,
}

impl SearchInput {
    /// 创建新实例
    pub fn new() -> Self {
        Self::default()
    }

    /// 从已有文本创建，光标置于末尾
    pub fn with_text(text: &str) -> Self {
        Self {
            cursor_pos: text.chars().count(),
            buffer: text.to_string(),
        }
    }

    // === 文本访问 ===

    /// 获取当前输入内容
    pub fn text(&self) -> &str {
        &self.buffer
    }

    /// 获取光标位置
    pub fn cursor_pos(&self) -> usize {
        self.cursor_pos
    }

    /// 输入是否为空
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 获取光标前后的文本（用于渲染）
    pub fn split_at_cursor(&self) -> (&str, &str) {
        let byte_pos = self.char_to_byte_pos(self.cursor_pos);
        (&self.buffer[..byte_pos], &self.buffer[byte_pos..])
    }

    // === 光标移动 ===

    /// 光标左移一个字符
    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// 光标右移一个字符
    pub fn move_right(&mut self) {
        let char_count = self.buffer.chars().count();
        if self.cursor_pos < char_count {
            self.cursor_pos += 1;
        }
    }

    /// 光标移到行首
    pub fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// 光标移到行尾
    pub fn move_end(&mut self) {
        self.cursor_pos = self.buffer.chars().count();
    }

    // === 文本编辑 ===

    /// 在光标位置插入字符
    pub fn insert(&mut self, c: char) {
        let byte_pos = self.char_to_byte_pos(self.cursor_pos);
        self.buffer.insert(byte_pos, c);
        self.cursor_pos += 1;
    }

    /// 删除光标前的字符 (Backspace)
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let char_indices: Vec<_> = self.buffer.char_indices().collect();
            if let Some(&(start, _)) = char_indices.get(self.cursor_pos - 1) {
                let end = char_indices
                    .get(self.cursor_pos)
                    .map(|&(i, _)| i)
                    .unwrap_or(self.buffer.len());
                self.buffer.drain(start..end);
                self.cursor_pos -= 1;
            }
        }
    }

    /// 删除光标处的字符 (Delete)
    pub fn delete(&mut self) {
        let char_count = self.buffer.chars().count();
        if self.cursor_pos < char_count {
            let char_indices: Vec<_> = self.buffer.char_indices().collect();
            if let Some(&(start, _)) = char_indices.get(self.cursor_pos) {
                let end = char_indices
                    .get(self.cursor_pos + 1)
                    .map(|&(i, _)| i)
                    .unwrap_or(self.buffer.len());
                self.buffer.drain(start..end);
            }
        }
    }

    /// 清空输入
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor_pos = 0;
    }

    /// 设置完整文本（光标移到末尾）
    pub fn set_text(&mut self, text: &str) {
        self.buffer = text.to_string();
        self.cursor_pos = self.buffer.chars().count();
    }

    /// 删除前一个单词 (Ctrl+W)
    pub fn delete_word(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }

        let chars: Vec<char> = self.buffer.chars().collect();
        let mut new_pos = self.cursor_pos;

        // 跳过光标前的空格
        while new_pos > 0 && chars[new_pos - 1].is_whitespace() {
            new_pos -= 1;
        }
        // 删除到单词开头
        while new_pos > 0 && !chars[new_pos - 1].is_whitespace() {
            new_pos -= 1;
        }

        // 删除 new_pos 到 cursor_pos 之间的字符
        let start_byte = self.char_to_byte_pos(new_pos);
        let end_byte = self.char_to_byte_pos(self.cursor_pos);
        self.buffer.drain(start_byte..end_byte);
        self.cursor_pos = new_pos;
    }

    // === 辅助方法 ===

    /// 将字符索引转换为字节索引
    fn char_to_byte_pos(&self, char_pos: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let input = SearchInput::new();
        assert!(input.is_empty());
        assert_eq!(input.cursor_pos(), 0);
    }

    #[test]
    fn test_with_text() {
        let input = SearchInput::with_text("hello");
        assert_eq!(input.text(), "hello");
        assert_eq!(input.cursor_pos(), 5);
    }

    #[test]
    fn test_insert() {
        let mut input = SearchInput::new();
        input.insert('a');
        input.insert('b');
        input.insert('c');
        assert_eq!(input.text(), "abc");
        assert_eq!(input.cursor_pos(), 3);
    }

    #[test]
    fn test_insert_at_cursor() {
        let mut input = SearchInput::with_text("ac");
        input.move_left(); // cursor at 'c'
        input.insert('b');
        assert_eq!(input.text(), "abc");
    }

    #[test]
    fn test_backspace() {
        let mut input = SearchInput::with_text("abc");
        input.backspace();
        assert_eq!(input.text(), "ab");
        assert_eq!(input.cursor_pos(), 2);
    }

    #[test]
    fn test_backspace_at_start() {
        let mut input = SearchInput::with_text("abc");
        input.move_home();
        input.backspace(); // should do nothing
        assert_eq!(input.text(), "abc");
    }

    #[test]
    fn test_delete() {
        let mut input = SearchInput::with_text("abc");
        input.move_home();
        input.delete();
        assert_eq!(input.text(), "bc");
    }

    #[test]
    fn test_cursor_movement() {
        let mut input = SearchInput::with_text("abc");
        assert_eq!(input.cursor_pos(), 3);

        input.move_left();
        assert_eq!(input.cursor_pos(), 2);

        input.move_home();
        assert_eq!(input.cursor_pos(), 0);

        input.move_right();
        assert_eq!(input.cursor_pos(), 1);

        input.move_end();
        assert_eq!(input.cursor_pos(), 3);
    }

    #[test]
    fn test_unicode() {
        let mut input = SearchInput::new();
        input.insert('你');
        input.insert('好');
        assert_eq!(input.text(), "你好");
        assert_eq!(input.cursor_pos(), 2);

        input.backspace();
        assert_eq!(input.text(), "你");
        assert_eq!(input.cursor_pos(), 1);
    }

    #[test]
    fn test_split_at_cursor() {
        let mut input = SearchInput::with_text("hello");
        input.move_home();
        input.move_right();
        input.move_right();

        let (before, after) = input.split_at_cursor();
        assert_eq!(before, "he");
        assert_eq!(after, "llo");
    }

    #[test]
    fn test_delete_word() {
        let mut input = SearchInput::with_text("hello world");
        input.delete_word();
        assert_eq!(input.text(), "hello ");

        input.delete_word();
        assert_eq!(input.text(), "");
    }

    #[test]
    fn test_clear() {
        let mut input = SearchInput::with_text("hello");
        input.clear();
        assert!(input.is_empty());
        assert_eq!(input.cursor_pos(), 0);
    }
}
