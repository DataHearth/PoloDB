const { Database, version } = require("..");
const path = require('path');
const os = require('os');
const { expect } = require("chai");
const fs = require('fs');

let temp;
let dbPath;

describe('Transaction', function() {
  let db;

  this.beforeAll(function() {
    if (temp === undefined) {
      temp = os.tmpdir()
      console.log('temp dir: ', temp);
    }
    dbPath = path.join(temp, 'test-db.db');
    if (fs.existsSync(dbPath)) {
      fs.unlinkSync(dbPath);
    }
    db = new Database(dbPath);
  });

  this.afterAll(function() {
    if (db) {
      db.close();
      db = null;
    }
  });

  it('commit', function() {
    db.startTransaction();
    let collection = db.createCollection('test-trans');
    expect(collection).to.not.be.undefined;
    collection.insert({
      _id: 3,
      name: "2333",
    });
    db.commit();
    db.close();

    db = new Database(dbPath);
    collection = db.collection('test-trans');
    const result = collection.find({
      name: "2333",
    });
    expect(result.length).to.equals(1);
  });

  it('rollback', function() {
    db.createCollection('test-trans');
    db.startTransaction();
    const collection = db.collection('test-trans');
    let result;
    result = collection.find({
      name: "rollback",
    })
    expect(result.length).to.equals(0);
    collection.insert({
      _id: 4,
      name: "rollback",
    });
    result = collection.find({
      name: "rollback",
    })
    expect(result.length).to.equals(1);
    db.rollback();
    result = collection.find({
      name: "rollback",
    });
    expect(result.length).to.equals(0);
  });

});
