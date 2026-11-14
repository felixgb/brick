#pragma once

#include <QDialog>

#include "Item.h"

class Core;
class ItemFields;
class ItemStore;
class QCheckBox;
class QLineEdit;

// Settings for the Claude integration: the API key and the model.
class ClaudeSettingsDialog : public QDialog {
    Q_OBJECT
public:
    explicit ClaudeSettingsDialog(Core *core, QWidget *parent = nullptr);

    static bool isConfigured(Core *core);

private:
    void save();

    Core *m_core;
    QLineEdit *m_key;
    QLineEdit *m_model;
};

// What Claude found in an email. Nothing has been saved yet: the user edits,
// unticks and confirms.
class ExtractionDialog : public QDialog {
    Q_OBJECT
public:
    ExtractionDialog(ItemStore *store, const QString &subject, const QString &summary, bool truncated,
                     const QList<Item> &proposals, QWidget *parent = nullptr);

private:
    struct Card {
        QCheckBox *include;
        ItemFields *fields;
    };

    ItemStore *m_store;
    QList<Card> m_cards;
};
