#ifndef MDVIEW_CLIPBOARD_H
#define MDVIEW_CLIPBOARD_H

#include <QApplication>
#include <QClipboard>
#include <QString>

// The only authored C++ in the shell: cxx cannot express QApplication's static factory, and
// QClipboard::text/setText are overloaded.
inline QString mdview_clipboard_text() {
    return QApplication::clipboard()->text();
}

inline void mdview_set_clipboard_text(const QString& text) {
    QApplication::clipboard()->setText(text);
}

#endif
